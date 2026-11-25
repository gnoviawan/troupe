//! The terminal's screen (`components/terminal/ConnectedTerminal.tsx` with xterm.js's
//! renderer): a `troupe_term::Term`'s rows drawn on one `rune::grid`, the cursor over it, and
//! the keys a focused terminal turns into PTY input ([`keys`]).
//!
//! Each row becomes one `GridRow`: narrow ASCII cells share one shaped line with colour spans,
//! every other glyph (wide CJK, emoji, box drawing, combining marks) is pinned to its column,
//! backgrounds are fills, and underline, strikethrough and overline are a vector layer. A row
//! is rebuilt only when its line's revision changes ([`RowCache`]), so a screen where one row
//! changed shapes that row only. Colours resolve here, as xterm.js does at draw time: bold
//! brightens the eight base colours, inverse swaps, dim halves the foreground's alpha.

pub mod keys;
mod rows;

#[cfg(test)]
mod rows_tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rune::{Color, El, GridRow, TextStyle, el, grid};
use theme::Tokens;
use troupe_term::{CursorShape, Term};

pub use rows::grid_row;

/// The React terminal's font size and row height factor (`terminal-config.ts`).
pub const FONT_SIZE: f32 = 14.0;
pub const LINE_HEIGHT: f32 = 1.2;
/// The terminal surface's padding (`px-4 py-0.5 pb-1`): left/right, top, bottom.
pub const PAD: [f32; 3] = [16.0, 2.0, 4.0];
/// The phone shell's horizontal padding (`px-1.5`).
pub const PAD_PHONE: f32 = 6.0;

/// The cell box and the text style rows draw with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub cell_w: f32,
    pub cell_h: f32,
    pub style: TextStyle,
}

impl Metrics {
    /// xterm.js's cell: the font's advance and its line box times the line height, each floored
    /// to a whole pixel.
    pub fn new(font_size: f32, line_height: f32) -> Metrics {
        let probe = TextStyle::new(font_size, 400.0).mono();
        let sample = "W".repeat(64);
        let cell_w = (rune::text::advance(&sample, probe) / 64.0).floor().max(1.0);
        let (_, natural) = rune::text::measure("W", &[], probe, None);
        let cell_h = (natural.ceil() * line_height).floor().max(1.0);
        Metrics { cell_w, cell_h, style: probe.lh(cell_h) }
    }

    /// The columns and rows that fit `width` x `height` points of content.
    pub fn fit(&self, width: f32, height: f32) -> (u16, u16) {
        let cols = (width / self.cell_w).floor().clamp(2.0, 1000.0) as u16;
        let rows = (height / self.cell_h).floor().clamp(1.0, 1000.0) as u16;
        (cols, rows)
    }
}

/// The theme's xterm colours, and the 256-colour palette xterm.js builds from them.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub fg: Color,
    pub bg: Color,
    pub cursor: Color,
    pub cursor_accent: Color,
    pub ansi: [Color; 256],
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r: f32::from(r) / 255.0, g: f32::from(g) / 255.0, b: f32::from(b) / 255.0, a: 1.0 }
}

impl Palette {
    pub fn new(t: &Tokens) -> Palette {
        let x = &t.terminal;
        let base = [
            x.black, x.red, x.green, x.yellow, x.blue, x.magenta, x.cyan, x.white, x.bright_black, x.bright_red,
            x.bright_green, x.bright_yellow, x.bright_blue, x.bright_magenta, x.bright_cyan, x.bright_white,
        ];
        let mut ansi = [Color::BLACK; 256];
        ansi[..16].copy_from_slice(&base);
        const STEPS: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
        for i in 0..216 {
            ansi[16 + i] = rgb(STEPS[i / 36], STEPS[(i / 6) % 6], STEPS[i % 6]);
        }
        for i in 0..24 {
            let v = (8 + i * 10) as u8;
            ansi[232 + i] = rgb(v, v, v);
        }
        Palette { fg: x.foreground, bg: x.background, cursor: x.cursor, cursor_accent: x.cursor_accent, ansi }
    }

    pub fn resolve(&self, c: troupe_term::Color, default: Color) -> Color {
        match c {
            troupe_term::Color::Default => default,
            troupe_term::Color::Indexed(i) => self.ansi[usize::from(i)],
            troupe_term::Color::Rgb(r, g, b) => rgb(r, g, b),
        }
    }
}

/// The rows a terminal drew last, by line revision. Hand the same cache back each frame.
#[derive(Default)]
pub struct RowCache {
    rows: HashMap<(u64, bool), Rc<GridRow>>,
    /// What the rows were built for: a different palette or metrics rebuilds them all.
    key: Option<(Palette, [u32; 2], usize)>,
}

/// What a terminal's screen shows.
pub struct Screen<'a> {
    pub term: &'a Term,
    /// Lines scrolled up into the scrollback (0: the live screen).
    pub scrolled: usize,
    pub focused: bool,
    /// The blink phase: false while a blinking cursor is off.
    pub blink_on: bool,
}

/// The visible rows of `screen` as grid rows, reusing `cache` for unchanged lines.
pub fn visible_rows(screen: &Screen, palette: &Palette, m: &Metrics, cache: &mut RowCache) -> Vec<Rc<GridRow>> {
    let term = screen.term;
    let key = (palette.clone(), [m.cell_w.to_bits(), m.cell_h.to_bits()], term.cols());
    if cache.key.as_ref() != Some(&key) {
        cache.rows.clear();
        cache.key = Some(key);
    }
    let lines = term.lines();
    let top = term.screen_start().saturating_sub(screen.scrolled);
    let mut kept = HashMap::with_capacity(term.rows());
    let rows = (top..top + term.rows())
        .filter_map(|i| Some((lines.get(i)?, lines.get(i + 1).is_some_and(|next| next.wrapped))))
        .map(|(line, wraps)| {
            let key = (line.rev, wraps);
            let row = cache.rows.remove(&key).unwrap_or_else(|| Rc::new(grid_row(line, wraps, term.cols(), palette, m)));
            kept.insert(key, row.clone());
            row
        })
        .collect();
    cache.rows = kept;
    rows
}

/// The terminal surface: the grid over the theme's terminal background, with the cursor.
pub fn screen(key: &str, screen: &Screen, palette: &Palette, m: &Metrics, cache: &RefCell<RowCache>) -> El {
    let term = screen.term;
    let rows = visible_rows(screen, palette, m, &mut cache.borrow_mut());
    let width = m.cell_w * term.cols() as f32;
    let height = m.cell_h * term.rows() as f32;
    let cursor = term.cursor();
    let on_screen = screen.scrolled == 0 && cursor.visible;
    let mut g = grid(term.cols() as u16, m.cell_w, m.cell_h, rows).key(format!("{key}-grid")).w(width).h(height);
    if on_screen {
        g = g.grid_caret(cursor.col.min(term.cols() - 1) as u16, cursor.row as u16);
    }
    let mut surface = el().key(key.to_owned()).w(width).h(height).child(g);
    if on_screen && (screen.blink_on || !screen.focused) {
        surface = surface.child(cursor_box(term, screen.focused, palette, m));
    }
    surface
}

/// xterm.js's cursor: a block with the character in the accent colour while focused, an
/// outline while not, or the bar and underline shapes a program asked for.
fn cursor_box(term: &Term, focused: bool, palette: &Palette, m: &Metrics) -> El {
    let c = term.cursor();
    let col = c.col.min(term.cols() - 1);
    let (x, y) = (col as f32 * m.cell_w, c.row as f32 * m.cell_h);
    let cell = term.screen_line(c.row).and_then(|l| l.cells.get(col));
    let wide = cell.is_some_and(|c| c.width == 2);
    let w = if wide { m.cell_w * 2.0 } else { m.cell_w };
    let shape = c.shape.unwrap_or(CursorShape::Block);
    let base = el().key("terminal-cursor").absolute(x, y);
    match (shape, focused) {
        (CursorShape::Block, true) => {
            let text = cell.map(troupe_term::Cell::text).filter(|t| !t.is_empty()).unwrap_or_default();
            base.w(w).h(m.cell_h).fill(palette.cursor).child(rune::text(text).font(m.style).color(palette.cursor_accent))
        }
        (CursorShape::Block, false) => base.w(w).h(m.cell_h).border(1.0, palette.cursor),
        (CursorShape::Bar, _) => base.w(1.0).h(m.cell_h).fill(palette.cursor),
        (CursorShape::Underline, _) => base.offset(0.0, m.cell_h - 1.0).w(w).h(1.0).fill(palette.cursor),
    }
}
