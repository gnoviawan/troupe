//! Cells and lines: what one column of a row shows and how a row's cells shift, as xterm.js's
//! `BufferLine` keeps them.

use std::sync::atomic::{AtomicU64, Ordering};

/// A cell's foreground or background colour as the program set it. The palette and the
/// default colours resolve at paint time, so a theme change recolours what is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    #[default]
    Default,
    /// An index into the 256-colour palette (0 to 15 are the theme's ANSI colours).
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// SGR attributes, a bit each, in the order the xterm cell dumps list them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(pub u16);

impl Flags {
    pub const BOLD: Flags = Flags(1);
    pub const DIM: Flags = Flags(1 << 1);
    pub const ITALIC: Flags = Flags(1 << 2);
    pub const UNDERLINE: Flags = Flags(1 << 3);
    pub const BLINK: Flags = Flags(1 << 4);
    pub const INVERSE: Flags = Flags(1 << 5);
    pub const INVISIBLE: Flags = Flags(1 << 6);
    pub const STRIKE: Flags = Flags(1 << 7);
    pub const OVERLINE: Flags = Flags(1 << 8);

    pub fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn set(&mut self, other: Flags, on: bool) {
        if on {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }
}

/// The underline's shape while [`Flags::UNDERLINE`] is set (`CSI 4:n m`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Underline {
    #[default]
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// The colours and attributes a cell draws with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
    pub underline: Underline,
}

impl Style {
    /// What erased cells and new lines take: the current background colour only
    /// (xterm.js's `_eraseAttrData`).
    pub fn erase(&self) -> Style {
        Style { bg: self.bg, ..Style::default() }
    }
}

/// One column of a row. A wide character takes its cell (width 2) and the next one, which
/// holds nothing and has width 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// The character, or `'\0'` for a blank cell (never written, or erased).
    pub ch: char,
    /// Combining marks drawn over `ch`.
    pub marks: Option<Box<str>>,
    pub width: u8,
    pub style: Style,
}

impl Cell {
    pub fn blank(style: Style) -> Cell {
        Cell { ch: '\0', marks: None, width: 1, style }
    }

    fn spacer(style: Style) -> Cell {
        Cell { ch: '\0', marks: None, width: 0, style }
    }

    /// What the cell reads as text: empty for a blank cell or a wide cell's right half.
    pub fn text(&self) -> String {
        let mut s = String::new();
        if self.ch != '\0' {
            s.push(self.ch);
        }
        if let Some(marks) = &self.marks {
            s.push_str(marks);
        }
        s
    }

    pub fn has_content(&self) -> bool {
        self.ch != '\0'
    }
}

static REV: AtomicU64 = AtomicU64::new(1);

/// A fresh line revision: unique across every terminal in the process.
fn next_rev() -> u64 {
    REV.fetch_add(1, Ordering::Relaxed)
}

/// One row of a buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub cells: Vec<Cell>,
    /// The terminal wrapped the line above into this one.
    pub wrapped: bool,
    /// Changes whenever the line does, so a view can keep what it built for a revision.
    pub rev: u64,
}

impl Line {
    pub fn new(cols: usize, fill: Style) -> Line {
        Line { cells: vec![Cell::blank(fill); cols], wrapped: false, rev: next_rev() }
    }

    pub(crate) fn touch(&mut self) {
        self.rev = next_rev();
    }

    fn width(&self, col: usize) -> u8 {
        self.cells.get(col).map_or(1, |c| c.width)
    }

    pub(crate) fn put(&mut self, col: usize, ch: char, width: u8, style: Style) {
        if let Some(c) = self.cells.get_mut(col) {
            *c = Cell { ch, marks: None, width, style };
        }
    }

    pub(crate) fn put_blank(&mut self, col: usize, style: Style) {
        self.put(col, '\0', 1, style);
    }

    pub(crate) fn put_spacer(&mut self, col: usize, style: Style) {
        if let Some(c) = self.cells.get_mut(col) {
            *c = Cell::spacer(style);
        }
    }

    pub(crate) fn add_mark(&mut self, col: usize, mark: char) {
        if let Some(c) = self.cells.get_mut(col) {
            let mut marks = c.marks.take().map(String::from).unwrap_or_default();
            marks.push(mark);
            c.marks = Some(marks.into_boxed_str());
        }
    }

    /// `BufferLine.insertCells`.
    pub(crate) fn insert_cells(&mut self, pos: usize, n: usize, fill: Style) {
        let len = self.cells.len();
        if len == 0 {
            return;
        }
        let pos = pos % len;
        if pos > 0 && self.width(pos - 1) == 2 {
            self.put_blank(pos - 1, fill);
        }
        if n < len - pos {
            self.cells[pos..].rotate_right(n);
            self.cells[pos..pos + n].fill(Cell::blank(fill));
        } else {
            self.cells[pos..].fill(Cell::blank(fill));
        }
        if self.width(len - 1) == 2 {
            self.put_blank(len - 1, fill);
        }
    }

    /// `BufferLine.deleteCells`.
    pub(crate) fn delete_cells(&mut self, pos: usize, n: usize, fill: Style) {
        let len = self.cells.len();
        if len == 0 {
            return;
        }
        let pos = pos % len;
        if n < len - pos {
            self.cells[pos..].rotate_left(n);
            self.cells[len - n..].fill(Cell::blank(fill));
        } else {
            self.cells[pos..].fill(Cell::blank(fill));
        }
        if pos > 0 && self.width(pos - 1) == 2 {
            self.put_blank(pos - 1, fill);
        }
        if self.width(pos) == 0 && !self.cells[pos].has_content() {
            self.put_blank(pos, fill);
        }
    }

    /// `BufferLine.replaceCells`: blanks `start..end` and the halves of wide cells it cuts.
    pub(crate) fn replace_cells(&mut self, start: usize, end: usize, fill: Style) {
        let len = self.cells.len();
        if start > 0 && self.width(start - 1) == 2 {
            self.put_blank(start - 1, fill);
        }
        if end < len && end > 0 && self.width(end - 1) == 2 {
            self.put_blank(end, fill);
        }
        let end = end.min(len);
        if start < end {
            self.cells[start..end].fill(Cell::blank(fill));
        }
    }

    pub(crate) fn fill(&mut self, cell: Cell) {
        self.cells.fill(cell);
    }

    pub(crate) fn resize(&mut self, cols: usize, fill: Style) {
        self.cells.resize(cols, Cell::blank(fill));
    }
}
