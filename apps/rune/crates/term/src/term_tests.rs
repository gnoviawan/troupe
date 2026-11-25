use crate::{Color, Flags, Term};

fn text(term: &Term, row: usize) -> String {
    term.screen_line(row).map(|l| l.cells.iter().map(|c| if c.ch == '\0' { ' ' } else { c.ch }).collect::<String>()).unwrap_or_default().trim_end().to_owned()
}

#[test]
fn prints_wraps_and_scrolls_into_scrollback() {
    let mut t = Term::with_scrollback(4, 2, 10);
    t.advance(b"abcdef\r\ngh\r\nij");
    assert_eq!(text(&t, 0), "gh");
    assert_eq!(text(&t, 1), "ij");
    assert_eq!(t.lines().len(), 4);
    assert!(t.lines()[1].wrapped, "ef continues abcd");
    assert_eq!(t.cursor().col, 2);
}

#[test]
fn sgr_sets_colours_and_flags() {
    let mut t = Term::new(10, 2);
    t.advance(b"\x1b[1;9;38;5;208;48;2;1;2;3mX");
    let c = &t.screen_line(0).unwrap().cells[0];
    assert_eq!(c.style.fg, Color::Indexed(208));
    assert_eq!(c.style.bg, Color::Rgb(1, 2, 3));
    assert!(c.style.flags.contains(Flags::BOLD) && c.style.flags.contains(Flags::STRIKE));
}

#[test]
fn wide_characters_take_two_cells() {
    let mut t = Term::new(10, 2);
    t.advance("中x".as_bytes());
    let cells = &t.screen_line(0).unwrap().cells;
    assert_eq!((cells[0].ch, cells[0].width), ('中', 2));
    assert_eq!(cells[1].width, 0);
    assert_eq!(cells[2].ch, 'x');
}

#[test]
fn the_alternate_screen_keeps_the_normal_one() {
    let mut t = Term::new(10, 3);
    t.advance(b"shell\x1b[?1049h\x1b[2Jvim");
    assert!(t.alt_screen());
    assert_eq!(text(&t, 0), "     vim");
    t.advance(b"\x1b[?1049l");
    assert_eq!(text(&t, 0), "shell");
}

#[test]
fn answers_cursor_reports() {
    let mut t = Term::new(10, 3);
    t.advance(b"ab\x1b[6n\x1b[c");
    assert_eq!(t.take_replies(), b"\x1b[1;3R\x1b[?1;2c");
}

#[test]
fn a_reset_keeps_the_answers_owed_before_it() {
    let mut t = Term::new(10, 3);
    t.advance(b"ab\x1b[6n\x1bc");
    assert_eq!(t.take_replies(), b"\x1b[1;3R");
    assert_eq!(text(&t, 0), "");
}

#[test]
fn reverse_wrap_backspace_changes_the_unwrapped_line() {
    let mut t = Term::new(4, 3);
    t.advance(b"abcdef\x1b[?45h");
    let before = t.screen_line(1).map(|l| l.rev);
    t.advance(b"\r\x08");
    let line = t.screen_line(1).expect("row 1");
    assert!(!line.wrapped);
    assert_ne!(Some(line.rev), before, "a line whose wrap flag cleared has a new revision");
}

#[test]
fn a_mark_after_a_resize_to_two_columns_does_not_underflow() {
    // A lone variation selector leaves a zero-width cell at column 0; the resize puts the
    // cursor on column 1 right after it, where a mark used to reach for column -1.
    let mut t = Term::new(4, 2);
    t.advance("\u{fe0f}a".as_bytes());
    t.resize(2, 2);
    t.advance("\u{301}".as_bytes());
    assert_eq!(t.cursor().col, 2);
    assert_eq!(text(&t, 0), "\u{fe0f}\u{301}");
}

#[test]
fn a_cell_keeps_a_bounded_run_of_marks() {
    let mut t = Term::new(10, 2);
    let mut bytes = b"e".to_vec();
    bytes.extend("\u{301}".repeat(100).as_bytes());
    t.advance(&bytes);
    let cell = &t.screen_line(0).unwrap().cells[0];
    assert_eq!(cell.marks.as_deref().map(|m| m.chars().count()), Some(crate::cell::MAX_MARKS));
    assert_eq!(t.cursor().col, 1);
}

/// Parse time for `n` marks on one cell, the best of three runs.
fn mark_run_secs(n: usize) -> f64 {
    let mut bytes = b"e".to_vec();
    bytes.extend("\u{301}".repeat(n).as_bytes());
    (0..3)
        .map(|_| {
            let mut t = Term::new(80, 24);
            let start = std::time::Instant::now();
            t.advance(&bytes);
            start.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_million_marks_parse_in_linear_time() {
    let small = mark_run_secs(100_000);
    let big = mark_run_secs(1_000_000);
    // Linear time reads about 10x; the quadratic append read over 1000x.
    assert!(big < small * 40.0, "100k marks {small:.3}s, 1M marks {big:.3}s");
}
