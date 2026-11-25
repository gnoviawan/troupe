//! The editor's keymap: CodeMirror's `defaultKeymap`, `historyKeymap` and `indentWithTab`, Mod-s
//! to save, and the find keys (`editorFindKeymap`: Mod-f, Mod-h, Ctrl-g, F3 and Mod-g), as
//! `use-codemirror.ts` stacks them. `Mod` is Ctrl off macOS and Cmd on it.

use rune::{Ime, Key, KeyEvent};
use troupe_editor::{Command, Motion};

use super::Editors;

/// What a key did that needs the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Not an editor key; it goes on to the window's shortcuts.
    Ignored,
    Handled,
    /// Mod-c or Mod-x: put this on the clipboard.
    Copy(String),
    /// Mod-v: read the clipboard and insert it.
    Paste,
}

impl Editors {
    /// One key in `path`'s editor.
    pub fn key(&mut self, path: &str, key: &KeyEvent) -> Outcome {
        let m = key.mods;
        let word = m.cmd || m.ctrl || m.alt;
        let page = self.panes.get(path).map_or(20, |p| p.page.max(1));
        if let Some(c) = command_char(key) {
            return self.chord(path, c, m.shift);
        }
        let command = match &key.key {
            Key::Text(t) if !(m.cmd || m.ctrl) => Command::Insert(t.clone()),
            Key::Enter => Command::Newline,
            Key::Backspace if word => Command::DeleteWordBackward,
            Key::Backspace => Command::Backspace,
            Key::Delete if word => Command::DeleteWordForward,
            Key::Delete => Command::Delete,
            Key::Tab if m.shift => Command::Outdent,
            Key::Tab => Command::Indent,
            Key::Left => moving(if word { Motion::WordLeft } else { Motion::Left }, m.shift),
            Key::Right => moving(if word { Motion::WordRight } else { Motion::Right }, m.shift),
            Key::Up => moving(Motion::Up, m.shift),
            Key::Down => moving(Motion::Down, m.shift),
            Key::Home if m.cmd || m.ctrl => moving(Motion::DocStart, m.shift),
            Key::End if m.cmd || m.ctrl => moving(Motion::DocEnd, m.shift),
            Key::Home => moving(Motion::LineStart, m.shift),
            Key::End => moving(Motion::LineEnd, m.shift),
            Key::PageUp => moving(Motion::PageUp(page), m.shift),
            Key::PageDown => moving(Motion::PageDown(page), m.shift),
            Key::Function(3) => {
                self.find_step(path, !m.shift);
                return Outcome::Handled;
            }
            Key::Escape => {
                let open = self.panes.get(path).is_some_and(|p| p.find.is_some() || p.goto.is_some());
                if !open {
                    return Outcome::Ignored;
                }
                self.close_find(path);
                return Outcome::Handled;
            }
            _ => return Outcome::Ignored,
        };
        self.apply(path, command);
        Outcome::Handled
    }

    fn chord(&mut self, path: &str, c: char, shift: bool) -> Outcome {
        match (c, shift) {
            ('a', _) => {
                self.apply(path, Command::SelectAll);
            }
            ('z', false) => {
                self.apply(path, Command::Undo);
            }
            ('z', true) | ('y', false) => {
                self.apply(path, Command::Redo);
            }
            ('s', _) => self.save(path),
            ('f', _) => self.open_find(path, false),
            ('h', _) => self.open_find(path, true),
            ('g', false) if cfg!(target_os = "macos") => self.find_step(path, true),
            ('g', _) => self.open_goto(path),
            ('c' | 'x', _) => {
                let Some(doc) = self.docs.docs.get(path) else { return Outcome::Ignored };
                let range = doc.editor.selection().range();
                if range.is_empty() {
                    return Outcome::Handled;
                }
                let text = doc.editor.rope().slice(range).to_string();
                if c == 'x' {
                    self.apply(path, Command::Insert(String::new()));
                }
                return Outcome::Copy(text);
            }
            ('v', _) => return Outcome::Paste,
            _ => return Outcome::Ignored,
        }
        Outcome::Handled
    }

    /// An input method event for the focused editor. Returns whether it was used.
    pub fn ime(&mut self, ime: &Ime) -> bool {
        let Some(path) = self.focused.clone() else {
            return false;
        };
        match ime {
            Ime::Preedit(text, ..) => {
                if let Some(pane) = self.panes.get_mut(&path) {
                    pane.preedit = (!text.is_empty()).then(|| text.clone());
                }
                true
            }
            Ime::Commit(text) => {
                if let Some(pane) = self.panes.get_mut(&path) {
                    pane.preedit = None;
                }
                self.apply(&path, Command::Insert(text.clone()));
                true
            }
            _ => false,
        }
    }
}

fn moving(motion: Motion, extend: bool) -> Command {
    Command::Move { motion, extend }
}

/// The letter of a Mod chord (`Key::Command`, or a typed letter with Ctrl held).
fn command_char(key: &KeyEvent) -> Option<char> {
    match &key.key {
        Key::Command(c) => Some(c.to_ascii_lowercase()),
        Key::Text(t) if key.mods.cmd || key.mods.ctrl => t.chars().next().map(|c| c.to_ascii_lowercase()),
        _ => None,
    }
}
