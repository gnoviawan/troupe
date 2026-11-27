//! A screen buffer as xterm.js's `Buffer` keeps it: one list of lines whose last `rows` are the
//! screen and the rest scrollback, the cursor, the scroll region, tab stops and the saved cursor.

use std::collections::VecDeque;

use crate::cell::{Cell, Line, Style};

/// What `DECSC` saves and `DECRC` restores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Saved {
    pub x: usize,
    /// An absolute line index, so a restore after scrolling lands where xterm.js puts it.
    pub y: usize,
    pub style: Style,
    pub charsets: [Charset; 4],
    pub glevel: usize,
    pub origin: bool,
    pub wraparound: bool,
}

/// A character set a G0 to G3 slot holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charset {
    #[default]
    Ascii,
    /// DEC special graphics (`ESC ( 0`): line drawing.
    DecGraphics,
    /// The United Kingdom set (`ESC ( A`): `#` reads `£`.
    Uk,
}

impl Charset {
    pub(crate) fn map(self, c: char) -> char {
        match self {
            Charset::Ascii => c,
            Charset::Uk => {
                if c == '#' {
                    '£'
                } else {
                    c
                }
            }
            Charset::DecGraphics => match c {
                '`' => '◆',
                'a' => '▒',
                'b' => '␉',
                'c' => '␌',
                'd' => '␍',
                'e' => '␊',
                'f' => '°',
                'g' => '±',
                'h' => '␤',
                'i' => '␋',
                'j' => '┘',
                'k' => '┐',
                'l' => '┌',
                'm' => '└',
                'n' => '┼',
                'o' => '⎺',
                'p' => '⎻',
                'q' => '─',
                'r' => '⎼',
                's' => '⎽',
                't' => '├',
                'u' => '┤',
                'v' => '┴',
                'w' => '┬',
                'x' => '│',
                'y' => '≤',
                'z' => '≥',
                '{' => 'π',
                '|' => '≠',
                '}' => '£',
                '~' => '·',
                _ => c,
            },
        }
    }
}

pub(crate) const TAB_WIDTH: usize = 4;

#[derive(Clone, Debug)]
pub(crate) struct Buffer {
    pub lines: VecDeque<Line>,
    /// The most lines kept: the screen plus the scrollback.
    pub cap: usize,
    pub x: usize,
    pub y: usize,
    pub top: usize,
    pub bottom: usize,
    pub tabs: Vec<bool>,
    pub saved: Saved,
    /// Lines dropped from the top of the scrollback so far.
    pub dropped: usize,
}

impl Buffer {
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Buffer {
        let mut b = Buffer {
            lines: VecDeque::new(),
            cap: rows + scrollback,
            x: 0,
            y: 0,
            top: 0,
            bottom: rows - 1,
            tabs: Vec::new(),
            saved: Saved { wraparound: true, ..Saved::default() },
            dropped: 0,
        };
        b.fill_rows(cols, rows, Style::default());
        b.reset_tabs(cols);
        b
    }

    /// The index in `lines` of the screen's first row (xterm.js's `ybase`).
    pub fn base(&self, rows: usize) -> usize {
        self.lines.len().saturating_sub(rows)
    }

    pub fn fill_rows(&mut self, cols: usize, rows: usize, fill: Style) {
        while self.lines.len() < rows {
            self.lines.push_back(Line::new(cols, fill));
        }
    }

    pub fn reset_tabs(&mut self, cols: usize) {
        self.tabs = (0..cols).map(|c| c % TAB_WIDTH == 0).collect();
    }

    pub fn next_stop(&self, cols: usize) -> usize {
        let mut x = self.x + 1;
        while x < cols && !self.tabs.get(x).copied().unwrap_or(false) {
            x += 1;
        }
        x.min(cols - 1)
    }

    pub fn prev_stop(&self, cols: usize) -> usize {
        let mut x = self.x.min(cols);
        while x > 0 {
            x -= 1;
            if self.tabs.get(x).copied().unwrap_or(false) {
                break;
            }
        }
        x.min(cols - 1)
    }

    /// `BufferService.scroll`: the region moves up a line and a blank `fill` line enters at its
    /// bottom. With the region's top at the screen's top the line that leaves goes to the
    /// scrollback, dropping the oldest line once the buffer is full.
    pub fn scroll(&mut self, cols: usize, rows: usize, fill: Style, wrapped: bool) {
        let base = self.base(rows);
        let (top, bottom) = (base + self.top, base + self.bottom);
        if self.top == 0 {
            // A full buffer recycles its oldest line instead of allocating one.
            let line = if self.lines.len() >= self.cap && bottom + 1 == self.lines.len() {
                let mut old = self.lines.pop_front().expect("a full buffer has lines");
                self.dropped += 1;
                old.cells.clear();
                old.cells.resize(cols, Cell::blank(fill));
                old.wrapped = wrapped;
                old.status = None;
                old.touch();
                self.lines.push_back(old);
                return;
            } else {
                let mut line = Line::new(cols, fill);
                line.wrapped = wrapped;
                line
            };
            self.lines.insert(bottom + 1, line);
            if self.lines.len() > self.cap {
                self.lines.pop_front();
                self.dropped += 1;
            }
        } else {
            let mut line = Line::new(cols, fill);
            line.wrapped = wrapped;
            self.lines.remove(top);
            self.lines.insert(bottom, line);
        }
    }

    /// `Buffer.resize` without reflow: rows come back from the scrollback when the screen grows
    /// and go to it when it shrinks below the cursor.
    pub fn resize(&mut self, old: (usize, usize), cols: usize, rows: usize, scrollback: usize) {
        let (old_cols, old_rows) = old;
        let fill = Style::default();
        if cols > old_cols {
            for line in &mut self.lines {
                line.resize(cols, fill);
                line.touch();
            }
        }
        let mut base = self.base(old_rows);
        let mut add_to_y = 0;
        if rows > old_rows {
            for _ in old_rows..rows {
                if self.lines.len() < rows + base {
                    if base > 0 && self.lines.len() <= base + self.y + add_to_y + 1 {
                        base -= 1;
                        add_to_y += 1;
                    } else {
                        self.lines.push_back(Line::new(cols, fill));
                    }
                }
            }
        } else {
            for _ in rows..old_rows {
                if self.lines.len() > rows + base {
                    if self.lines.len() > base + self.y + 1 {
                        self.lines.pop_back();
                    } else {
                        base += 1;
                    }
                }
            }
        }
        let cap = rows + scrollback;
        if self.lines.len() > cap {
            let trim = self.lines.len() - cap;
            self.lines.drain(..trim);
            self.dropped += trim;
            self.saved.y = self.saved.y.saturating_sub(trim);
        }
        self.cap = cap;
        self.x = self.x.min(cols - 1);
        self.y = self.y.min(rows - 1) + add_to_y;
        self.saved.x = self.saved.x.min(cols - 1);
        self.top = 0;
        self.bottom = rows - 1;
        if cols < old_cols {
            for line in &mut self.lines {
                line.resize(cols, fill);
                line.touch();
            }
        }
        // The screen is the last `rows` lines; a buffer shorter than that grows at the bottom.
        self.fill_rows(cols, rows, fill);
        let max_y = self.lines.len().saturating_sub(self.base(rows)).saturating_sub(1);
        self.y = self.y.min(max_y);
        if self.tabs.len() != cols {
            let old = self.tabs.len();
            self.tabs.resize(cols, false);
            for c in old..cols {
                self.tabs[c] = c % TAB_WIDTH == 0;
            }
        }
    }
}
