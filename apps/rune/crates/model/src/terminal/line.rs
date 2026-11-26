//! The command editor's text and its keys (`components/terminal/command-editor/editor-keys.ts`):
//! a [`troupe_editor::Editor`] with the readline keys, Enter to send, Shift+Enter for a line
//! break, and Arrow Up and Down that walk the command history from the first and last line.
//! `tools/parity/vectors/command-keys.ts` records the keys against CodeMirror.

use troupe_editor::{Command, Editor, Motion, Selection};

use super::history::Recall;
use super::words::sanitize_paste;

/// A key the editor's keymap owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    ShiftEnter,
    Up,
    Down,
    /// Ctrl+A: the line's start.
    LineStart,
    /// Ctrl+E: the line's end.
    LineEnd,
    /// Ctrl+K: delete to the line's end, or the line break there.
    KillToEnd,
    /// Ctrl+U: delete to the line's start, or the line break there.
    KillToStart,
    /// Ctrl+W: delete the group before the caret.
    KillWord,
    /// Ctrl+C: copy a selection, else clear the draft.
    Interrupt,
    /// Ctrl+D: end of input on an empty draft, else delete forward.
    Eof,
    Undo,
    Redo,
}

/// What a key asks of the terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// The keymap passed on the key.
    Ignored,
    Handled,
    Submit(String),
    /// Write `^D` to the PTY.
    SendEof,
    Copy(String),
}

#[derive(Clone)]
pub struct Line {
    pub editor: Editor,
    /// The history walk Arrow Up began; an edit that is not a recall ends it.
    recall: Option<Recall>,
}

impl Line {
    /// An editor over the terminal's draft, the caret at its end, as React mounts it.
    #[must_use]
    pub fn new(draft: &str) -> Line {
        let mut editor = Editor::new(draft);
        let end = editor.rope().len_chars();
        editor.apply(Command::Select(Selection::caret(end)), 0.0);
        Line { editor, recall: None }
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.editor.text()
    }

    /// The caret as UTF-16 code units, as the host counts it.
    #[must_use]
    pub fn caret_utf16(&self) -> u64 {
        let head = self.editor.selection().head;
        self.editor.rope().chars().take(head).map(|c| c.len_utf16() as u64).sum()
    }

    /// An edit or a move the keymap does not own (typing, Backspace, the arrows across a line).
    /// A change to the text ends a history walk. Returns whether anything changed.
    pub fn apply(&mut self, command: Command, now: f64) -> bool {
        let revision = self.editor.revision();
        let changed = self.editor.apply(command, now);
        if self.editor.revision() != revision {
            self.recall = None;
        }
        changed
    }

    /// Pasted text, without bracketed-paste markers and the controls the editor refuses.
    pub fn paste(&mut self, text: &str, now: f64) -> bool {
        let clean = sanitize_paste(text);
        !clean.is_empty() && self.apply(Command::Insert(clean), now)
    }

    /// The whole text replaced (a recall or the Ctrl+C wipe), the caret at its end. It does not
    /// end a history walk.
    fn replace_all(&mut self, text: &str, now: f64) {
        let len = self.editor.rope().len_chars();
        self.editor.replace(0..len, text, now);
    }

    fn selected(&self) -> Option<String> {
        let range = self.editor.selection().range();
        (!range.is_empty()).then(|| self.editor.rope().slice(range).to_string())
    }

    fn line_bounds(&self) -> (usize, usize) {
        let rope = self.editor.rope();
        let line = rope.char_to_line(self.editor.selection().head);
        (rope.line_to_char(line), rope.line_to_char(line) + troupe_editor::line_len(rope, line))
    }

    fn delete(&mut self, range: std::ops::Range<usize>, now: f64) -> Action {
        if range.is_empty() {
            return Action::Ignored;
        }
        let at = range.start;
        self.editor.replace(range, "", now);
        self.editor.apply(Command::Select(Selection::caret(at)), now);
        self.recall = None;
        Action::Handled
    }

    /// `deleteToLineEnd` / `deleteToLineStart`: the selection, else to the line's edge, else
    /// the line break at the caret.
    fn kill(&mut self, forward: bool, now: f64) -> Action {
        let sel = self.editor.selection();
        if !sel.is_empty() {
            return self.delete(sel.range(), now);
        }
        let (start, end) = self.line_bounds();
        let len = self.editor.rope().len_chars();
        let head = sel.head;
        let range = match forward {
            true if head < end => head..end,
            true => head..(head + 1).min(len),
            false if head > start => start..head,
            false => head.saturating_sub(1)..head,
        };
        self.delete(range, now)
    }

    fn caret_to(&mut self, at: usize, now: f64) -> Action {
        self.editor.apply(Command::Select(Selection::caret(at)), now);
        Action::Handled
    }

    /// Arrow Up (`-1`) or Down (`1`): across the draft's lines first, then through `history`
    /// (oldest first) for entries that start with the walk's first line.
    fn walk(&mut self, history: &[String], direction: i64, now: f64) -> Action {
        let (line, _) = self.editor.line_col();
        let lines = self.editor.len_lines();
        if direction < 0 && line > 1 {
            self.editor.apply(Command::Move { motion: Motion::Up, extend: false }, now);
            return Action::Handled;
        }
        if direction > 0 && line < lines {
            self.editor.apply(Command::Move { motion: Motion::Down, extend: false }, now);
            return Action::Handled;
        }
        if self.recall.is_none() && direction > 0 {
            return Action::Ignored;
        }
        let mut walk = self.recall.take().unwrap_or_else(|| Recall::begin(&self.text()));
        let Some((text, going)) = walk.step(history, direction) else {
            if walk.offset >= 0 {
                self.recall = Some(walk);
            }
            return Action::Handled;
        };
        self.replace_all(&text, now);
        if going {
            self.recall = Some(walk);
        }
        Action::Handled
    }

    /// One key of the keymap. `history` is the project's sent commands, oldest first;
    /// `terminal_selection` the text selected in the terminal above.
    pub fn key(&mut self, key: Key, history: &[String], terminal_selection: &str, now: f64) -> Action {
        match key {
            Key::Enter => Action::Submit(self.text()),
            Key::ShiftEnter => {
                self.apply(Command::Insert("\n".into()), now);
                Action::Handled
            }
            Key::Up => self.walk(history, -1, now),
            Key::Down => self.walk(history, 1, now),
            Key::LineStart => self.caret_to(self.line_bounds().0, now),
            Key::LineEnd => self.caret_to(self.line_bounds().1, now),
            Key::KillToEnd => self.kill(true, now),
            Key::KillToStart => self.kill(false, now),
            Key::KillWord => match self.apply(Command::DeleteWordBackward, now) {
                true => Action::Handled,
                false => Action::Ignored,
            },
            Key::Interrupt => {
                if let Some(text) = self.selected() {
                    return Action::Copy(text);
                }
                if !terminal_selection.is_empty() {
                    return Action::Copy(terminal_selection.to_owned());
                }
                self.replace_all("", now);
                Action::Handled
            }
            Key::Eof => {
                let len = self.editor.rope().len_chars();
                if len == 0 {
                    return Action::SendEof;
                }
                if self.editor.selection().head < len {
                    self.apply(Command::Delete, now);
                }
                Action::Handled
            }
            Key::Undo => {
                self.apply(Command::Undo, now);
                Action::Handled
            }
            Key::Redo => {
                self.apply(Command::Redo, now);
                Action::Handled
            }
        }
    }
}
