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
