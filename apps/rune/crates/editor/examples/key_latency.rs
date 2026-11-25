//! Times what one typed key costs the editor model on a 1 MB Rust file: the insert (with its
//! undo step) and the highlight of the 32-line chunk on screen, which the view lexes again
//! after an edit. 200 keys at the end of the first line; prints p50, p95 and max in ms.

use std::time::Instant;

use troupe_editor::highlight::Highlights;
use troupe_editor::{Command, Editor, Motion};

fn main() {
    let mut text = String::new();
    let mut i = 0;
    while text.len() < 1_000_000 {
        text.push_str(&format!("fn large_marker_{i}(value: u64) -> u64 {{ value.wrapping_mul({i}) + 1 }} // line {i}\n"));
        i += 1;
    }
    let mut editor = Editor::new(&text);
    editor.apply(Command::Move { motion: Motion::LineEnd, extend: false }, 0.0);
    let mut highlights = Highlights::for_path("large.rs");
    let mut ms = Vec::new();
    for k in 0..200 {
        let start = Instant::now();
        editor.apply(Command::Insert("x".into()), k as f64);
        highlights.chunk(editor.rope(), 0..32);
        ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    ms.sort_by(f64::total_cmp);
    let at = |p: f64| ms[((p / 100.0) * ms.len() as f64) as usize];
    println!("{} bytes, {} keys: p50 {:.3} ms, p95 {:.3} ms, max {:.3} ms", text.len(), ms.len(), at(50.0), at(95.0), ms[ms.len() - 1]);
}
