//! Times the parse of `seq 1 200000` output (1,288,895 bytes) in 64 KiB chunks, the shape a PTY
//! hands it over in: `cargo run --release -p troupe-term --example drain -- 120 40`.

use std::time::Instant;

fn main() {
    let args: Vec<u16> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let (cols, rows) = (args.first().copied().unwrap_or(120), args.get(1).copied().unwrap_or(40));
    let bytes: Vec<u8> = (1..=200_000).flat_map(|n| format!("{n}\r\n").into_bytes()).collect();
    let mut runs = Vec::new();
    for _ in 0..5 {
        let mut term = troupe_term::Term::new(cols, rows);
        let start = Instant::now();
        for chunk in bytes.chunks(64 * 1024) {
            term.advance(chunk);
        }
        runs.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(term.screen_line(rows as usize - 2).map(|l| l.cells[0].ch), Some('2'));
    }
    runs.sort_by(f64::total_cmp);
    println!("{} bytes at {cols}x{rows}: median {:.1} ms, runs {:?}", bytes.len(), runs[2], runs.iter().map(|r| format!("{r:.1}")).collect::<Vec<_>>());
}
