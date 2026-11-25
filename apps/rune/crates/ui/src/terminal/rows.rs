//! One terminal line as a `GridRow`, with xterm.js's draw-time colour rules.

use rune::{Color, GridRow, LineCap, Shape, Span, TextStyle};
use troupe_term::{Cell, Flags, Line, Underline};

use super::{Metrics, Palette};

/// How a cell draws once its colours are resolved.
#[derive(Clone, Copy, PartialEq)]
struct Ink {
    fg: Color,
    bg: Option<Color>,
    bold: bool,
    italic: bool,
    underline: Option<Underline>,
    strike: bool,
    overline: bool,
    hidden: bool,
}

fn ink(cell: &Cell, p: &Palette) -> Ink {
    let s = &cell.style;
    let f = s.flags;
    let bold = f.contains(Flags::BOLD);
    // xterm.js's drawBoldTextInBrightColors: bold lifts the eight base colours to bright.
    let fg_index = match s.fg {
        troupe_term::Color::Indexed(i) if bold && i < 8 => troupe_term::Color::Indexed(i + 8),
        c => c,
    };
    let mut fg = p.resolve(fg_index, p.fg);
    let mut bg = p.resolve(s.bg, p.bg);
    let mut has_bg = s.bg != troupe_term::Color::Default;
    if f.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
        has_bg = true;
    }
    if f.contains(Flags::DIM) {
        fg = fg.alpha(0.5);
    }
    Ink {
        fg,
        bg: has_bg.then_some(bg),
        bold,
        italic: f.contains(Flags::ITALIC),
        underline: f.contains(Flags::UNDERLINE).then_some(s.underline),
        strike: f.contains(Flags::STRIKE),
        overline: f.contains(Flags::OVERLINE),
        hidden: f.contains(Flags::INVISIBLE),
    }
}

/// Whether the shared line can draw `c`: the mono font's ASCII sits exactly on the cell grid,
/// every other glyph is pinned to its column so a different advance cannot shift the row.
fn in_line(c: char) -> bool {
    (' '..='~').contains(&c)
}

fn style_of(m: &Metrics, ink: &Ink) -> TextStyle {
    let mut style = if ink.bold { TextStyle::new(m.style.size, 700.0).mono().lh(m.cell_h) } else { m.style };
    if ink.italic {
        style = style.italic();
    }
    style
}

/// The decorations of columns `start..end` drawn as strokes in the row's points.
fn decorate(shape: Shape, start: usize, end: usize, ink: &Ink, m: &Metrics) -> Shape {
    let (x0, x1) = (start as f32 * m.cell_w, end as f32 * m.cell_w);
    let h = m.cell_h;
    let line = |s: Shape, y: f32| s.line([x0, y], [x1, y], 1.0, LineCap::Butt, ink.fg);
    let mut shape = shape;
    match ink.underline {
        Some(Underline::Single) => shape = line(shape, h - 2.5),
        Some(Underline::Double) => shape = line(line(shape, h - 3.5), h - 1.5),
        Some(Underline::Curly) => {
            let points: Vec<[f32; 2]> = (0..=((x1 - x0) / 2.0) as usize)
                .map(|i| [x0 + i as f32 * 2.0, if i % 2 == 0 { h - 3.5 } else { h - 1.5 }])
                .collect();
            shape = shape.stroke(points, 1.0, ink.fg);
        }
        Some(Underline::Dotted | Underline::Dashed) => {
            let step = if ink.underline == Some(Underline::Dotted) { 2.0 } else { 4.0 };
            let mut x = x0;
            while x < x1 {
                shape = shape.line([x, h - 2.5], [(x + step / 2.0).min(x1), h - 2.5], 1.0, LineCap::Butt, ink.fg);
                x += step;
            }
        }
        None => {}
    }
    if ink.strike {
        shape = line(shape, (h / 2.0).round() + 0.5);
    }
    if ink.overline {
        shape = line(shape, 0.5);
    }
    shape
}

/// `line` as a grid row `cols` wide; `wraps` when the terminal wrapped it into the next line.
pub fn grid_row(line: &Line, wraps: bool, cols: usize, p: &Palette, m: &Metrics) -> GridRow {
    let mut text = String::with_capacity(cols);
    let mut spans: Vec<Span> = Vec::new();
    let mut pinned: Vec<(usize, u16, String, TextStyle, Color)> = Vec::new();
    let mut fills: Vec<(usize, usize, Color)> = Vec::new();
    let mut shape = Shape::new();
    let mut run: Option<(usize, Ink)> = None;
    let mut deco: Option<(usize, Ink)> = None;
    let cells = &line.cells[..cols.min(line.cells.len())];
    let flush_span = |spans: &mut Vec<Span>, start: usize, end: usize, ink: &Ink| {
        if end > start && (ink.fg != p.fg || ink.bold || ink.italic) {
            let mut span = Span::new(start, end).color(ink.fg);
            if ink.bold {
                span = span.bold();
            }
            if ink.italic {
                span = span.italic();
            }
            spans.push(span);
        }
    };
    for (col, cell) in cells.iter().enumerate() {
        let ink = ink(cell, p);
        let visible = cell.has_content() && !ink.hidden;
        // Fills: one per run of the same background.
        match (fills.last_mut(), ink.bg) {
            (Some((_, end, color)), Some(bg)) if *end == col && *color == bg => *end = col + 1,
            (_, Some(bg)) => fills.push((col, col + 1, bg)),
            _ => {}
        }
        let decorated = ink.underline.is_some() || ink.strike || ink.overline;
        let same_deco = deco.as_ref().is_some_and(|(_, d)| d.fg == ink.fg && d.underline == ink.underline && d.strike == ink.strike && d.overline == ink.overline);
        if !same_deco {
            if let Some((start, d)) = deco.take() {
                shape = decorate(shape, start, col, &d, m);
            }
            if decorated {
                deco = Some((col, ink));
            }
        }
        let same_run = run.as_ref().is_some_and(|(_, r)| r.fg == ink.fg && r.bold == ink.bold && r.italic == ink.italic);
        if !same_run {
            if let Some((start, r)) = run.take() {
                flush_span(&mut spans, start, col, &r);
            }
            run = Some((col, ink));
        }
        if cell.width == 0 || !visible {
            text.push(' ');
        } else if cell.width == 1 && cell.marks.is_none() && in_line(cell.ch) {
            text.push(cell.ch);
        } else {
            text.push(' ');
            pinned.push((col, u16::from(cell.width.max(1)), cell.text(), style_of(m, &ink), ink.fg));
        }
    }
    let end = cells.len();
    if let Some((start, r)) = run.take() {
        flush_span(&mut spans, start, end, &r);
    }
    if let Some((start, d)) = deco.take() {
        shape = decorate(shape, start, end, &d, m);
    }
    let mut row = GridRow::new(text, spans, m.style).wraps(wraps);
    for (start, end, color) in fills {
        row = row.fill(start as u16, end as u16, color);
    }
    for (col, width, text, style, color) in pinned {
        row = row.cell(col as u16, width, text, style, color);
    }
    row.shape(shape)
}
