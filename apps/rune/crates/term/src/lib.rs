//! Troupe's terminal cell model for the Rune client.
//!
//! [`Term`] takes the bytes a PTY printed and keeps what the React terminal (xterm.js) keeps for
//! them: a screen of [`Cell`]s with colours and attributes, scrollback, the alternate screen, the
//! cursor and the modes a program set. `vte` parses the escape sequences; every operation
//! follows the xterm.js method it is named after, and `tests/cells.rs` checks the cells against
//! dumps recorded from `@xterm/headless`. The crate has no UI and no I/O and compiles for wasm.

#![forbid(unsafe_code)]

mod buffer;
mod cell;
pub mod links;
mod perform;
pub mod search;
pub mod selection;
mod term;
pub mod text;
mod width;
mod width_table;

use std::collections::VecDeque;

pub use buffer::Charset;
pub use cell::{Cell, Color, Flags, Line, Style, Underline};
pub use term::{Cursor, CursorShape, Modes, MouseEncoding, MouseProtocol};
pub use width::width;

use term::Emu;

/// The scrollback the React terminal keeps (`terminal-config.ts`).
pub const SCROLLBACK: usize = 10_000;

/// A terminal's cells, fed from its PTY output.
pub struct Term {
    parser: vte::Parser,
    emu: Emu,
}

impl Term {
    /// A `cols` x `rows` terminal with [`SCROLLBACK`] lines of scrollback.
    pub fn new(cols: u16, rows: u16) -> Term {
        Term::with_scrollback(cols, rows, SCROLLBACK)
    }

    pub fn with_scrollback(cols: u16, rows: u16, scrollback: usize) -> Term {
        let (cols, rows) = clamp(cols, rows);
        Term { parser: vte::Parser::new(), emu: Emu::new(cols, rows, scrollback) }
    }

    /// Runs PTY output through the parser. A sequence split across calls continues.
    pub fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.emu, bytes);
    }

    /// Resizes the screen, as xterm.js does without reflowing wrapped lines.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = clamp(cols, rows);
        if (cols, rows) != (self.emu.cols, self.emu.rows) {
            self.emu.resize(cols, rows);
        }
    }

    pub fn cols(&self) -> usize {
        self.emu.cols
    }

    pub fn rows(&self) -> usize {
        self.emu.rows
    }

    /// The active buffer's lines: the scrollback first (none on the alternate screen), then the
    /// screen's rows from [`Term::screen_start`].
    pub fn lines(&self) -> &VecDeque<Line> {
        self.emu.lines()
    }

    /// The index in [`Term::lines`] of the screen's top row.
    pub fn screen_start(&self) -> usize {
        self.emu.buf().base(self.emu.rows)
    }

    /// The screen row `row` (0 at the top).
    pub fn screen_line(&self, row: usize) -> Option<&Line> {
        self.lines().get(self.screen_start() + row)
    }

    /// Lines the active buffer has dropped from the top of its scrollback so far. A position in
    /// [`Term::lines`] taken earlier moves up by the difference.
    pub fn dropped(&self) -> usize {
        self.emu.buf().dropped
    }

    pub fn alt_screen(&self) -> bool {
        self.emu.alt_active
    }

    /// The cursor. Its column can equal [`Term::cols`] after printing in the last column, until
    /// the next character wraps.
    pub fn cursor(&self) -> Cursor {
        let b = self.emu.buf();
        Cursor {
            col: b.x,
            row: b.y,
            visible: !self.emu.cursor_hidden,
            shape: self.emu.cursor_shape,
            blink: self.emu.cursor_blink,
        }
    }

    pub fn modes(&self) -> &Modes {
        &self.emu.modes
    }

    /// The window title the program set (OSC 0 or 2).
    pub fn title(&self) -> &str {
        &self.emu.title
    }

    /// Bytes the terminal answers the program with (device attributes, cursor reports), to write
    /// to the PTY.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.emu.replies)
    }
}

/// xterm.js needs two columns and a row.
fn clamp(cols: u16, rows: u16) -> (usize, usize) {
    (usize::from(cols.max(2)), usize::from(rows.max(1)))
}

#[cfg(test)]
mod term_tests;
