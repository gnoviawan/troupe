use crate::{Command, Editor, Motion, Selection};

const SOURCE: &str = "fn main() {\n    let total = add(1, 2);\n    println!(\"{total}\");\n}\n\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";

/// A splitmix64 stream, so the recorded edits are the same on every run.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// `count` edits of every kind the keymap sends, with caret moves and selections between them.
fn record(seed: u64, count: usize) -> Vec<Command> {
    let mut s = Stream(seed);
    let words = ["x", "let ", "é", "日本", "{", "}", "\t", "  ", "fn f() {}", "// note"];
    let moves = [
        Motion::Left,
        Motion::Right,
        Motion::WordLeft,
        Motion::WordRight,
        Motion::Up,
        Motion::Down,
        Motion::LineStart,
        Motion::LineEnd,
        Motion::DocStart,
        Motion::DocEnd,
        Motion::PageDown(3),
        Motion::PageUp(3),
    ];
    let mut out = Vec::new();
    while out.len() < count {
        out.push(Command::Move { motion: moves[s.below(moves.len() as u64) as usize], extend: s.below(4) == 0 });
        let edit = match s.below(9) {
            0 | 1 => Command::Insert(words[s.below(words.len() as u64) as usize].to_owned()),
            2 => Command::Newline,
            3 => Command::Backspace,
            4 => Command::Delete,
            5 => Command::DeleteWordBackward,
            6 => Command::DeleteWordForward,
            7 => Command::Indent,
            _ => Command::Outdent,
        };
        out.push(edit);
    }
    out
}

fn is_edit(command: &Command) -> bool {
    !matches!(command, Command::Move { .. } | Command::Select(_) | Command::SelectAll)
}

#[test]
fn a_thousand_recorded_edits_undo_back_to_the_original() {
    let mut ed = Editor::new(SOURCE);
    let commands: Vec<Command> = record(7, 2000);
    let mut edits = 0;
    let mut now = 0.0;
    for command in commands {
        if edits == 1000 {
            break;
        }
        if is_edit(&command) {
            edits += 1;
            now += 1.0;
        }
        ed.apply(command, now);
    }
    assert_eq!(edits, 1000);
    let edited = ed.text();
    assert_ne!(edited, SOURCE);
    let mut undone = 0;
    while ed.can_undo() {
        assert!(ed.apply(Command::Undo, now));
        undone += 1;
    }
    assert_eq!(ed.text(), SOURCE);
    assert!(undone > 0 && undone <= 1000, "{undone} steps");
    while ed.can_redo() {
        ed.apply(Command::Redo, now);
    }
    assert_eq!(ed.text(), edited);
}

#[test]
fn typing_within_half_a_second_is_one_undo_step() {
    let mut ed = Editor::new("");
    for (i, c) in "hello".chars().enumerate() {
        ed.apply(Command::Insert(c.to_string()), i as f64 * 0.1);
    }
    ed.apply(Command::Insert(" world".into()), 0.5);
    assert_eq!(ed.text(), "hello world");
    ed.apply(Command::Undo, 1.0);
    assert_eq!(ed.text(), "hello");
    ed.apply(Command::Undo, 1.0);
    assert_eq!(ed.text(), "");
    assert!(!ed.can_undo());
    ed.apply(Command::Redo, 1.0);
    assert_eq!(ed.text(), "hello");
}

#[test]
fn a_pause_starts_a_new_undo_step() {
    let mut ed = Editor::new("");
    ed.apply(Command::Insert("a".into()), 0.0);
    ed.apply(Command::Insert("b".into()), 2.0);
    ed.apply(Command::Undo, 3.0);
    assert_eq!(ed.text(), "a");
}

#[test]
fn undo_puts_the_caret_back() {
    let mut ed = Editor::new("abc");
    ed.apply(Command::Move { motion: Motion::DocEnd, extend: false }, 0.0);
    ed.apply(Command::Backspace, 0.0);
    assert_eq!(ed.selection(), Selection::caret(2));
    ed.apply(Command::Undo, 1.0);
    assert_eq!((ed.text().as_str(), ed.selection()), ("abc", Selection::caret(3)));
}

#[test]
fn line_breaks_load_as_codemirror_reads_them() {
    let ed = Editor::new("a\r\nb\rc\n");
    assert_eq!(ed.text(), "a\nb\nc\n");
    assert_eq!(ed.len_lines(), 4);
}

#[test]
fn enter_keeps_the_indent_and_opens_brackets() {
    let mut ed = Editor::new("    let x = 1;");
    ed.apply(Command::Move { motion: Motion::DocEnd, extend: false }, 0.0);
    ed.apply(Command::Newline, 0.0);
    assert_eq!(ed.text(), "    let x = 1;\n    ");
    let mut ed = Editor::new("fn f() {}");
    ed.apply(Command::Select(Selection::caret(8)), 0.0);
    ed.apply(Command::Newline, 0.0);
    assert_eq!(ed.text(), "fn f() {\n  \n}");
    assert_eq!(ed.line_col(), (2, 3));
}

#[test]
fn tab_indents_every_selected_line_and_shift_tab_takes_it_back() {
    let mut ed = Editor::new("a\nb\nc");
    ed.apply(Command::Select(Selection { anchor: 0, head: 3 }), 0.0);
    ed.apply(Command::Indent, 0.0);
    assert_eq!(ed.text(), "  a\n  b\nc");
    ed.apply(Command::Outdent, 1.0);
    assert_eq!(ed.text(), "a\nb\nc");
    let mut ed = Editor::new("x");
    ed.apply(Command::Indent, 0.0);
    assert_eq!((ed.text().as_str(), ed.selection()), ("  x", Selection::caret(2)));
}

#[test]
fn word_moves_stop_where_codemirror_groups_end() {
    let mut ed = Editor::new("let total = add(1, 2);");
    let mut stops = Vec::new();
    while ed.apply(Command::Move { motion: Motion::WordRight, extend: false }, 0.0) {
        stops.push(ed.selection().head);
    }
    assert_eq!(stops, [3, 9, 11, 15, 16, 17, 18, 20, 22]);
    ed.apply(Command::DeleteWordBackward, 0.0);
    assert_eq!(ed.text(), "let total = add(1, 2");
}

#[test]
fn home_goes_to_the_indent_then_the_line_start() {
    let mut ed = Editor::new("    x = 1");
    ed.apply(Command::Move { motion: Motion::LineEnd, extend: false }, 0.0);
    ed.apply(Command::Move { motion: Motion::LineStart, extend: false }, 0.0);
    assert_eq!(ed.selection().head, 4);
    ed.apply(Command::Move { motion: Motion::LineStart, extend: false }, 0.0);
    assert_eq!(ed.selection().head, 0);
}

#[test]
fn up_and_down_keep_the_goal_column_and_follow_wrapped_rows() {
    let mut ed = Editor::new("abcdef\nx\nabcdef");
    ed.apply(Command::Select(Selection::caret(5)), 0.0);
    ed.apply(Command::Move { motion: Motion::Down, extend: false }, 0.0);
    assert_eq!(ed.line_col(), (2, 2));
    ed.apply(Command::Move { motion: Motion::Down, extend: false }, 0.0);
    assert_eq!(ed.line_col(), (3, 6));
    ed.apply(Command::Move { motion: Motion::Down, extend: false }, 0.0);
    assert_eq!(ed.selection().head, ed.rope().len_chars());

    let mut ed = Editor::new("aaaa bbbb cccc");
    ed.set_wrap(5);
    ed.apply(Command::Select(Selection::caret(1)), 0.0);
    ed.apply(Command::Move { motion: Motion::Down, extend: false }, 0.0);
    assert_eq!(ed.selection().head, 6);
    ed.apply(Command::Move { motion: Motion::Down, extend: false }, 0.0);
    assert_eq!(ed.selection().head, 11);
}

#[test]
fn arrows_collapse_a_selection_to_its_side() {
    let mut ed = Editor::new("hello");
    ed.apply(Command::Select(Selection { anchor: 1, head: 4 }), 0.0);
    ed.apply(Command::Move { motion: Motion::Left, extend: false }, 0.0);
    assert_eq!(ed.selection(), Selection::caret(1));
}

#[test]
fn typing_replaces_the_selection() {
    let mut ed = Editor::new("hello world");
    ed.apply(Command::Select(Selection { anchor: 6, head: 11 }), 0.0);
    ed.apply(Command::Insert("rope".into()), 0.0);
    assert_eq!(ed.text(), "hello rope");
    assert_eq!(ed.line_col(), (1, 11));
}

#[test]
fn go_to_line_offsets_clamp_into_the_text() {
    let ed = Editor::new("ab\ncde\n");
    assert_eq!(ed.offset_of(2, 2), 4);
    assert_eq!(ed.offset_of(2, 99), 6);
    assert_eq!(ed.offset_of(99, 1), 7);
}

#[test]
fn a_reload_keeps_the_caret_and_undoes_in_one_step() {
    let mut ed = Editor::new("one\ntwo\n");
    ed.apply(Command::Select(Selection::caret(5)), 0.0);
    ed.reset("one\ntwo\nthree\n", 1.0);
    assert_eq!((ed.text().as_str(), ed.selection()), ("one\ntwo\nthree\n", Selection::caret(5)));
    ed.apply(Command::Undo, 2.0);
    assert_eq!(ed.text(), "one\ntwo\n");
}
