//! Box-drawing and block characters drawn as shapes, as xterm.js's `customGlyphs` draws them,
//! so lines join across cells and rows instead of leaving the font's gaps.
//!
//! A line character is four arms from the cell's centre (up, right, down, left), each none,
//! light, heavy or double. Block elements are rectangles in eighths of the cell, and the three
//! shades fill the cell at a quarter, a half and three quarters of the ink.

use rune::{Color, Path, Shape};

/// An arm's weight.
const N: u8 = 0;
const L: u8 = 1;
const H: u8 = 2;
const D: u8 = 3;

/// The arms of a box-drawing line character: up, right, down, left.
fn arms(c: char) -> Option<[u8; 4]> {
    Some(match c {
        '─' => [N, L, N, L],
        '━' => [N, H, N, H],
        '│' => [L, N, L, N],
        '┃' => [H, N, H, N],
        '┌' => [N, L, L, N],
        '┍' => [N, H, L, N],
        '┎' => [N, L, H, N],
        '┏' => [N, H, H, N],
        '┐' => [N, N, L, L],
        '┑' => [N, N, L, H],
        '┒' => [N, N, H, L],
        '┓' => [N, N, H, H],
        '└' => [L, L, N, N],
        '┕' => [L, H, N, N],
        '┖' => [H, L, N, N],
        '┗' => [H, H, N, N],
        '┘' => [L, N, N, L],
        '┙' => [L, N, N, H],
        '┚' => [H, N, N, L],
        '┛' => [H, N, N, H],
        '├' => [L, L, L, N],
        '┝' => [L, H, L, N],
        '┠' => [H, L, H, N],
        '┣' => [H, H, H, N],
        '┤' => [L, N, L, L],
        '┥' => [L, N, L, H],
        '┨' => [H, N, H, L],
        '┫' => [H, N, H, H],
        '┬' => [N, L, L, L],
        '┯' => [N, H, L, H],
        '┰' => [N, L, H, L],
        '┳' => [N, H, H, H],
        '┴' => [L, L, N, L],
        '┷' => [L, H, N, H],
        '┸' => [H, L, N, L],
        '┻' => [H, H, N, H],
        '┼' => [L, L, L, L],
        '┿' => [L, H, L, H],
        '╂' => [H, L, H, L],
        '╋' => [H, H, H, H],
        '╴' => [N, N, N, L],
        '╵' => [L, N, N, N],
        '╶' => [N, L, N, N],
        '╷' => [N, N, L, N],
        '╸' => [N, N, N, H],
        '╹' => [H, N, N, N],
        '╺' => [N, H, N, N],
        '╻' => [N, N, H, N],
        '╼' => [N, H, N, L],
        '╽' => [L, N, H, N],
        '╾' => [N, L, N, H],
        '╿' => [H, N, L, N],
        '═' => [N, D, N, D],
        '║' => [D, N, D, N],
        '╒' => [N, D, L, N],
        '╓' => [N, L, D, N],
        '╔' => [N, D, D, N],
        '╕' => [N, N, L, D],
        '╖' => [N, N, D, L],
        '╗' => [N, N, D, D],
        '╘' => [L, D, N, N],
        '╙' => [D, L, N, N],
        '╚' => [D, D, N, N],
        '╛' => [L, N, N, D],
        '╜' => [D, N, N, L],
        '╝' => [D, N, N, D],
        '╞' => [L, D, L, N],
        '╟' => [D, L, D, N],
        '╠' => [D, D, D, N],
        '╡' => [L, N, L, D],
        '╢' => [D, N, D, L],
        '╣' => [D, N, D, D],
        '╤' => [N, D, L, D],
        '╥' => [N, L, D, L],
        '╦' => [N, D, D, D],
        '╧' => [L, D, N, D],
        '╨' => [D, L, N, L],
        '╩' => [D, D, N, D],
        '╪' => [L, D, L, D],
        '╫' => [D, L, D, L],
        '╬' => [D, D, D, D],
        _ => return None,
    })
}

/// A block element as rectangles in eighths of the cell: `(x0, y0, x1, y1)` and an alpha.
fn blocks(c: char) -> Option<(Vec<[u8; 4]>, f32)> {
    let rect = |r: [u8; 4]| (vec![r], 1.0);
    Some(match c {
        '▀' => rect([0, 0, 8, 4]),
        '▁'..='▇' => rect([0, 8 - (c as u32 - 0x2580) as u8, 8, 8]),
        '█' => rect([0, 0, 8, 8]),
        '▉'..='▏' => rect([0, 0, 8 - (c as u32 - 0x2588) as u8, 8]),
        '▐' => rect([4, 0, 8, 8]),
        '░' => (vec![[0, 0, 8, 8]], 0.25),
        '▒' => (vec![[0, 0, 8, 8]], 0.5),
        '▓' => (vec![[0, 0, 8, 8]], 0.75),
        '▔' => rect([0, 0, 8, 1]),
        '▕' => rect([7, 0, 8, 8]),
        '▖' => rect([0, 4, 4, 8]),
        '▗' => rect([4, 4, 8, 8]),
        '▘' => rect([0, 0, 4, 4]),
        '▝' => rect([4, 0, 8, 4]),
        '▙' => (vec![[0, 0, 4, 8], [4, 4, 8, 8]], 1.0),
        '▚' => (vec![[0, 0, 4, 4], [4, 4, 8, 8]], 1.0),
        '▛' => (vec![[0, 0, 8, 4], [0, 4, 4, 8]], 1.0),
        '▜' => (vec![[0, 0, 8, 4], [4, 4, 8, 8]], 1.0),
        '▞' => (vec![[4, 0, 8, 4], [0, 4, 4, 8]], 1.0),
        '▟' => (vec![[4, 0, 8, 4], [0, 4, 8, 8]], 1.0),
        _ => return None,
    })
}

/// Whether [`draw`] draws `c` itself.
pub fn custom(c: char) -> bool {
    arms(c).is_some() || blocks(c).is_some() || matches!(c, '╭' | '╮' | '╯' | '╰' | '╱' | '╲' | '╳')
}

fn rect(shape: Shape, x0: f32, y0: f32, x1: f32, y1: f32, color: Color) -> Shape {
    shape.fill(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], color)
}

/// Draws `c` into the cell at `x` that is `w` x `h` points, in `color`.
pub fn draw(shape: Shape, c: char, x: f32, w: f32, h: f32, color: Color) -> Shape {
    // The centre lines sit on whole points so a light line is one crisp pixel at scale 1.
    let (cx, cy) = ((x + w / 2.0).floor(), (h / 2.0).floor());
    let (right, bottom) = (x + w, h);
    if let Some(a) = arms(c) {
        let mut s = shape;
        // Up, right, down, left: each arm is one band (light, heavy) or two (double) from the
        // edge to the centre. How far past the centre a band reaches (in points, negative
        // short of it) depends on the arms across it, so corners and junctions join as
        // xterm.js draws them: a double line stops at the crossing arm's near line when that
        // arm is double, and runs to its far line otherwise.
        for (i, &weight) in a.iter().enumerate() {
            if weight == N {
                continue;
            }
            let through = a[(i + 2) % 4] != N;
            // The perpendicular arms on the side of the band nearer 0 (up or left), then the other.
            let (first, second) = if i % 2 == 1 { (a[0], a[2]) } else { (a[3], a[1]) };
            let double = |near: u8, far: u8| if near == D { -1.0 } else if through || far == D { 1.0 } else { 0.0 };
            let single = if (first == D || second == D) && !through {
                -1.0
            } else if first == H || second == H {
                1.0
            } else {
                0.0
            };
            let bands: [(f32, f32, f32); 2] = match weight {
                L => [(0.0, 1.0, single), (0.0, 0.0, 0.0)],
                H => [(-1.0, 2.0, single), (0.0, 0.0, 0.0)],
                _ => [(-1.0, 0.0, double(first, second)), (1.0, 2.0, double(second, first))],
            };
            for (lo, hi, t) in bands.into_iter().filter(|b| b.1 > b.0) {
                s = match i {
                    0 => rect(s, cx + lo, 0.0, cx + hi, cy + t + 1.0, color),
                    1 => rect(s, cx - t, cy + lo, right, cy + hi, color),
                    2 => rect(s, cx + lo, cy - t, cx + hi, bottom, color),
                    _ => rect(s, x, cy + lo, cx + t + 1.0, cy + hi, color),
                };
            }
        }
        return s;
    }
    if let Some((rects, alpha)) = blocks(c) {
        let ink = color.alpha(color.a * alpha);
        let (ex, ey) = (w / 8.0, h / 8.0);
        return rects.into_iter().fold(shape, |s, [x0, y0, x1, y1]| {
            rect(s, x + f32::from(x0) * ex, f32::from(y0) * ey, x + f32::from(x1) * ex, f32::from(y1) * ey, ink)
        });
    }
    let (mx, my) = (cx + 0.5, cy + 0.5);
    let arc = |from: [f32; 2], to: [f32; 2]| Path::new().move_to(from).quad_to([mx, my], to);
    match c {
        '╭' => shape.stroke_path(&arc([mx, bottom], [right, my]), 1.0, rune::LineCap::Butt, color),
        '╮' => shape.stroke_path(&arc([mx, bottom], [x, my]), 1.0, rune::LineCap::Butt, color),
        '╯' => shape.stroke_path(&arc([mx, 0.0], [x, my]), 1.0, rune::LineCap::Butt, color),
        '╰' => shape.stroke_path(&arc([mx, 0.0], [right, my]), 1.0, rune::LineCap::Butt, color),
        '╱' => shape.line([right, 0.0], [x, bottom], 1.0, rune::LineCap::Butt, color),
        '╲' => shape.line([x, 0.0], [right, bottom], 1.0, rune::LineCap::Butt, color),
        '╳' => shape
            .line([right, 0.0], [x, bottom], 1.0, rune::LineCap::Butt, color)
            .line([x, 0.0], [right, bottom], 1.0, rune::LineCap::Butt, color),
        _ => shape,
    }
}
