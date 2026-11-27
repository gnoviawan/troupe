//! What a terminal screen costs a frame on `rune::grid`: view, layout, shaping and the frame's
//! instances, without the GPU. A 200x60 screen is redrawn with every row changed, then with one
//! row changed, then with nothing changed.
//!
//!   cargo run --release -p ui --example grid_cost

use std::cell::{Cell, RefCell};
use std::time::Instant;

use rune::{Cx, El, Env, Runtime};
use troupe_term::Term;
use ui::terminal::{FONT_SIZE, LINE_HEIGHT, Metrics, Palette, RowCache, Screen, screen};

const COLS: u16 = 200;
const ROWS: u16 = 60;
const FRAMES: usize = 120;

struct S {
    term: Term,
    cache: RefCell<RowCache>,
    /// Measured in the first view, once the runtime's fonts measure text.
    metrics: Cell<Option<Metrics>>,
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    let t = theme::tokens("termul", true);
    let m = s.metrics.get().unwrap_or_else(|| Metrics::new(FONT_SIZE, LINE_HEIGHT));
    s.metrics.set(Some(m));
    let shown = Screen { term: &s.term, scrolled: 0, focused: true, blink_on: true, marks: &[], compact: false };
    screen("term", &shown, &Palette::new(t), &m, &s.cache)
}

/// A full screen of coloured words, different for every `n`.
fn full_screen(n: usize) -> Vec<u8> {
    let mut out = String::from("\x1b[H");
    for row in 0..usize::from(ROWS) {
        out.push_str(&format!("\x1b[{};1H", row + 1));
        let mut col = 0;
        let mut word = 0;
        while col + 12 < usize::from(COLS) {
            let text = format!("w{:05}-{:04} ", n % 100_000, row * 100 + word);
            out.push_str(&format!("\x1b[3{}m{text}", (row + word + n) % 8));
            col += text.len();
            word += 1;
        }
        out.push_str("\x1b[m");
    }
    out.into_bytes()
}

fn time(rt: &mut Runtime<S>, change: impl Fn(usize) -> Option<Vec<u8>>) -> (f64, f64) {
    let mut ms = Vec::with_capacity(FRAMES);
    for i in 0..FRAMES {
        if let Some(bytes) = change(i) {
            rt.update(|s, _| s.term.advance(&bytes));
        } else {
            rt.update(|_, _| {});
        }
        let start = Instant::now();
        rt.frame(1.0 / 60.0);
        ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    ms.sort_by(f64::total_cmp);
    (ms[ms.len() / 2], ms[ms.len() * 9 / 10])
}

fn main() {
    let state = S { term: Term::new(COLS, ROWS), cache: RefCell::default(), metrics: Cell::new(None) };
    let text = rune::text::TextSystem::for_window(theme::FONTS, theme::FAMILIES);
    let mut rt = Runtime::with_text(state, view, (1700.0, 1200.0), Env::new(true, 1.0), text);
    rt.update(|s, _| s.term.advance(&full_screen(0)));
    rt.frame(0.0);
    let full = time(&mut rt, |i| Some(full_screen(i + 1)));
    let one = time(&mut rt, |i| Some(format!("\x1b[30;1H\x1b[32mrow {i:06}\x1b[m").into_bytes()));
    let none = time(&mut rt, |_| None);
    let m = rt.state.metrics.get().expect("the view ran");
    println!("{COLS}x{ROWS} cells at {}x{} pt per cell, {FRAMES} frames each", m.cell_w, m.cell_h);
    for (name, (p50, p90)) in [("every row changed", full), ("one row changed", one), ("nothing changed", none)] {
        println!("{name:>18}: p50 {p50:.2} ms, p90 {p90:.2} ms");
    }
}
