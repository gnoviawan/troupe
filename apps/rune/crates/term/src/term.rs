//! The terminal: two buffers, the pen, the modes, and the operations escape sequences run.
//! Each operation follows the xterm.js `InputHandler` method it is named after, so the cells
//! match the React terminal's.

use std::collections::VecDeque;

use crate::buffer::{Buffer, Charset};
use crate::cell::{Cell, Line, Style};
use crate::width::width;

/// The cursor shape a program asked for with `DECSCUSR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    Bar,
}

/// Which pointer events a program wants reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseProtocol {
    #[default]
    None,
    X10,
    Vt200,
    Drag,
    Any,
}

/// How pointer reports are encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseEncoding {
    #[default]
    Default,
    Sgr,
    SgrPixels,
}

/// The modes a program toggles, as xterm.js's `term.modes` reports them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modes {
    pub application_cursor_keys: bool,
    pub application_keypad: bool,
    pub bracketed_paste: bool,
    pub insert: bool,
    pub origin: bool,
    pub reverse_wraparound: bool,
    pub send_focus: bool,
    pub synchronized_output: bool,
    pub wraparound: bool,
    pub mouse: MouseProtocol,
    pub mouse_encoding: MouseEncoding,
}

impl Default for Modes {
    fn default() -> Self {
        Modes {
            application_cursor_keys: false,
            application_keypad: false,
            bracketed_paste: false,
            insert: false,
            origin: false,
            reverse_wraparound: false,
            send_focus: false,
            synchronized_output: false,
            wraparound: true,
            mouse: MouseProtocol::None,
            mouse_encoding: MouseEncoding::Default,
        }
    }
}

/// Where the cursor is and how it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub col: usize,
    /// The screen row, 0 at the top of the screen.
    pub row: usize,
    pub visible: bool,
    /// `None` until a program sets one; the terminal's setting applies.
    pub shape: Option<CursorShape>,
    pub blink: Option<bool>,
}

pub(crate) struct Emu {
    pub cols: usize,
    pub rows: usize,
    pub scrollback: usize,
    pub normal: Buffer,
    pub alt: Buffer,
    pub alt_active: bool,
    pub pen: Style,
    pub modes: Modes,
    pub charsets: [Charset; 4],
    pub glevel: usize,
    pub cursor_hidden: bool,
    pub cursor_shape: Option<CursorShape>,
    pub cursor_blink: Option<bool>,
    pub title: String,
    pub replies: Vec<u8>,
    /// The width of the last character printed in this run (xterm.js's join state), or 0 after
    /// any other action: a zero-width character after it combines onto it.
    pub preceding: u8,
}

impl Emu {
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Emu {
        let mut alt = Buffer::new(cols, rows, 0);
        alt.lines.clear();
        Emu {
            cols,
            rows,
            scrollback,
            normal: Buffer::new(cols, rows, scrollback),
            alt,
            alt_active: false,
            pen: Style::default(),
            modes: Modes::default(),
            charsets: [Charset::Ascii; 4],
            glevel: 0,
            cursor_hidden: false,
            cursor_shape: None,
            cursor_blink: None,
            title: String::new(),
            replies: Vec::new(),
            preceding: 0,
        }
    }

    pub fn buf(&self) -> &Buffer {
        if self.alt_active { &self.alt } else { &self.normal }
    }

    pub fn buf_mut(&mut self) -> &mut Buffer {
        if self.alt_active { &mut self.alt } else { &mut self.normal }
    }

    /// The screen row `y`, marked changed.
    pub fn row(&mut self, y: usize) -> &mut Line {
        let rows = self.rows;
        let b = self.buf_mut();
        let i = b.base(rows) + y;
        let line = &mut b.lines[i];
        line.touch();
        line
    }

    pub fn erase_style(&self) -> Style {
        self.pen.erase()
    }

    fn scroll(&mut self, wrapped: bool) {
        let (cols, rows, fill) = (self.cols, self.rows, self.erase_style());
        self.buf_mut().scroll(cols, rows, fill, wrapped);
    }

    pub fn restrict_cursor(&mut self, max_col: usize) {
        let (rows, origin) = (self.rows, self.modes.origin);
        let b = self.buf_mut();
        b.x = b.x.min(max_col);
        b.y = if origin { b.y.clamp(b.top, b.bottom) } else { b.y.min(rows - 1) };
    }

    pub fn set_cursor(&mut self, x: isize, y: isize) {
        let origin = self.modes.origin;
        let b = self.buf_mut();
        let y = if origin { b.top as isize + y } else { y };
        b.x = x.max(0) as usize;
        b.y = y.max(0) as usize;
        self.restrict_cursor(self.cols - 1);
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        self.restrict_cursor(self.cols - 1);
        let (x, y) = (self.buf().x as isize, self.buf().y as isize);
        self.set_cursor(x + dx, y + dy);
    }

    /// `InputHandler.print` for one character, with the clean-ups xterm.js runs around a
    /// printed run done around each character, which leaves the same cells.
    pub fn print(&mut self, c: char) {
        let c = if (c as u32) < 127 { self.charsets[self.glevel].map(c) } else { c };
        if c == '\u{ad}' {
            return;
        }
        self.print_start();
        let mut w = width(c);
        let join = w == 0 && self.preceding != 0;
        let old_width = if join { self.preceding } else { 0 };
        if join && old_width > w {
            w = old_width;
        }
        self.preceding = w;
        let (cols, rows, pen) = (self.cols, self.rows, self.pen);
        let x = self.buf().x;
        if x + usize::from(w) > cols + usize::from(old_width) {
            if self.modes.wraparound {
                let y = self.buf().y;
                let line = self.row(y);
                for col in x..cols {
                    line.put_blank(col, pen);
                }
                let b = self.buf_mut();
                b.x = 0;
                b.y += 1;
                if b.y == b.bottom + 1 {
                    b.y -= 1;
                    self.scroll(true);
                } else {
                    if b.y >= rows {
                        b.y = rows - 1;
                    }
                    let y = b.y;
                    self.row(y).wrapped = true;
                }
            } else {
                self.buf_mut().x = cols - 1;
                if w == 2 {
                    return;
                }
            }
        }
        let (x, y) = (self.buf().x, self.buf().y);
        if join && x > 0 {
            let offset = if self.row_width(x - 1) != 0 { 1 } else { 2 };
            let line = self.row(y);
            line.add_mark(x - offset, c);
            for col in x..x + usize::from(w - old_width) {
                line.put_spacer(col, pen);
            }
            self.buf_mut().x += usize::from(w - old_width);
            return;
        }
        let insert = self.modes.insert;
        let line = self.row(y);
        if insert {
            line.insert_cells(x, usize::from(w), pen);
            if line.cells[cols - 1].width == 2 {
                line.put_blank(cols - 1, pen);
            }
        }
        line.put(x, c, w, pen);
        for col in x + 1..x + usize::from(w.max(1)) {
            line.put_spacer(col, pen);
        }
        self.buf_mut().x = x + usize::from(w.max(1));
        self.print_end();
    }

    fn row_width(&self, x: usize) -> u8 {
        let b = self.buf();
        let line = &b.lines[b.base(self.rows) + b.y];
        line.cells.get(x).map_or(1, |c| c.width)
    }

    /// Before and after a printed run, as xterm.js's `print` does around its loop.
    pub fn print_start(&mut self) {
        let x = self.buf().x;
        if x > 0 && x <= self.cols && self.row_width(x - 1) == 2 {
            let (y, pen) = (self.buf().y, self.pen);
            self.row(y).put_blank(x - 1, pen);
        }
    }

    pub fn print_end(&mut self) {
        let (x, y, pen) = (self.buf().x, self.buf().y, self.pen);
        if x < self.cols {
            let b = self.buf();
            let cell = &b.lines[b.base(self.rows) + y].cells[x];
            if cell.width == 0 && !cell.has_content() {
                self.row(y).put_blank(x, pen);
            }
        }
    }

    pub fn line_feed(&mut self) {
        let rows = self.rows;
        let b = self.buf_mut();
        b.y += 1;
        if b.y == b.bottom + 1 {
            b.y -= 1;
            self.scroll(false);
        } else if b.y >= rows {
            b.y = rows - 1;
        } else {
            let y = b.y;
            self.row(y).wrapped = false;
        }
        let cols = self.cols;
        let b = self.buf_mut();
        if b.x >= cols {
            b.x -= 1;
        }
    }

    pub fn backspace(&mut self) {
        if !self.modes.reverse_wraparound {
            self.restrict_cursor(self.cols - 1);
            let b = self.buf_mut();
            b.x = b.x.saturating_sub(1);
            return;
        }
        self.restrict_cursor(self.cols);
        let (cols, rows) = (self.cols, self.rows);
        let b = self.buf_mut();
        if b.x > 0 {
            b.x -= 1;
        } else {
            let base = b.base(rows);
            if b.y > b.top && b.y <= b.bottom && b.lines[base + b.y].wrapped {
                let line = &mut b.lines[base + b.y];
                line.wrapped = false;
                line.touch();
                b.y -= 1;
                b.x = cols - 1;
                let cell = &b.lines[base + b.y].cells[b.x];
                if cell.width != 0 && !cell.has_content() {
                    b.x -= 1;
                }
            }
        }
        self.restrict_cursor(self.cols - 1);
    }

    pub fn tab(&mut self) {
        let cols = self.cols;
        let b = self.buf_mut();
        if b.x < cols {
            b.x = b.next_stop(cols);
        }
    }

    pub fn index(&mut self) {
        self.restrict_cursor(self.cols - 1);
        let rows = self.rows;
        let b = self.buf_mut();
        b.y += 1;
        if b.y == b.bottom + 1 {
            b.y -= 1;
            self.scroll(false);
        } else if b.y >= rows {
            b.y = rows - 1;
        }
        self.restrict_cursor(self.cols - 1);
    }

    pub fn reverse_index(&mut self) {
        self.restrict_cursor(self.cols - 1);
        let (cols, rows, fill) = (self.cols, self.rows, self.erase_style());
        let b = self.buf_mut();
        if b.y == b.top {
            let base = b.base(rows);
            b.lines.remove(base + b.bottom);
            b.lines.insert(base + b.y, Line::new(cols, fill));
        } else {
            b.y = b.y.saturating_sub(1);
            self.restrict_cursor(self.cols - 1);
        }
    }

    pub fn erase_in_line(&mut self, y: usize, start: usize, end: usize, clear_wrap: bool) {
        let fill = self.erase_style();
        let line = self.row(y);
        line.replace_cells(start, end, fill);
        if clear_wrap {
            line.wrapped = false;
        }
    }

    pub fn reset_line(&mut self, y: usize) {
        let fill = self.erase_style();
        let line = self.row(y);
        line.fill(Cell::blank(fill));
        line.wrapped = false;
    }

    /// The region's lines `at..` shift down `n` (a blank `fill` line enters at `at`, the
    /// region's last line leaves), or up when `up`.
    pub fn shift_region(&mut self, at: usize, n: usize, up: bool, fill: Style) {
        let (cols, rows) = (self.cols, self.rows);
        let b = self.buf_mut();
        let base = b.base(rows);
        for _ in 0..n {
            if up {
                b.lines.remove(base + at);
                b.lines.insert(base + b.bottom, Line::new(cols, fill));
            } else {
                b.lines.remove(base + b.bottom);
                b.lines.insert(base + at, Line::new(cols, fill));
            }
        }
    }

    pub fn save_cursor(&mut self) {
        let rows = self.rows;
        let (pen, charsets, glevel, origin, wrap) =
            (self.pen, self.charsets, self.glevel, self.modes.origin, self.modes.wraparound);
        let b = self.buf_mut();
        b.saved.x = b.x;
        b.saved.y = b.base(rows) + b.y;
        b.saved.style = pen;
        b.saved.charsets = charsets;
        b.saved.glevel = glevel;
        b.saved.origin = origin;
        b.saved.wraparound = wrap;
    }

    pub fn restore_cursor(&mut self) {
        let rows = self.rows;
        let b = self.buf_mut();
        b.x = b.saved.x;
        b.y = b.saved.y.saturating_sub(b.base(rows));
        let s = b.saved.clone();
        self.pen.fg = s.style.fg;
        self.pen.bg = s.style.bg;
        self.pen.flags = s.style.flags;
        self.charsets = s.charsets;
        self.glevel = s.glevel;
        self.modes.origin = s.origin;
        self.modes.wraparound = s.wraparound;
        self.restrict_cursor(self.cols - 1);
    }

    pub fn activate_alt(&mut self) {
        if self.alt_active {
            return;
        }
        let (cols, rows, fill) = (self.cols, self.rows, self.erase_style());
        self.alt.fill_rows(cols, rows, fill);
        self.alt.x = self.normal.x;
        self.alt.y = self.normal.y;
        self.alt_active = true;
    }

    pub fn activate_normal(&mut self) {
        if !self.alt_active {
            return;
        }
        self.normal.x = self.alt.x;
        self.normal.y = self.alt.y;
        let (cols, rows) = (self.cols, self.rows);
        self.alt = Buffer::new(cols, rows, 0);
        self.alt.lines.clear();
        self.alt_active = false;
    }

    pub fn reply(&mut self, text: &str) {
        self.replies.extend_from_slice(text.as_bytes());
    }

    pub fn full_reset(&mut self) {
        let title = std::mem::take(&mut self.title);
        let replies = std::mem::take(&mut self.replies);
        *self = Emu::new(self.cols, self.rows, self.scrollback);
        self.title = title;
        self.replies = replies;
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let old = (self.cols, self.rows);
        self.normal.resize(old, cols, rows, self.scrollback);
        if self.alt_active {
            self.alt.resize(old, cols, rows, 0);
        } else {
            self.alt = Buffer::new(cols, rows, 0);
            self.alt.lines.clear();
        }
        self.cols = cols;
        self.rows = rows;
    }

    pub fn lines(&self) -> &VecDeque<Line> {
        &self.buf().lines
    }
}
