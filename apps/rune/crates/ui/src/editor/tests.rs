use model::editor::{Draft, Effect, Event, Read, sha256_hex};
use rune::{Ime, Key, KeyEvent, Mods};

use super::{Editors, Outcome};

const PATH: &str = "/p/main.rs";

fn loaded(text: &str, draft: Option<Draft>, disk_time: f64) -> Editors {
    let mut e = Editors::default();
    e.open(PATH, draft);
    let read = Read { content: text.to_owned(), modified_at: disk_time, content_hash: Some(sha256_hex(text)) };
    e.step(Event::Loaded { path: PATH.into(), result: Ok(read) });
    e.effects.clear();
    e
}

fn key(key: Key, mods: Mods) -> KeyEvent {
    KeyEvent { key, mods }
}

fn ctrl(c: char) -> KeyEvent {
    key(Key::Command(c), Mods { cmd: true, ..Mods::default() })
}

fn text(e: &Editors) -> String {
    e.docs.docs[PATH].editor.text()
}

#[test]
fn typing_edits_and_ctrl_s_writes_with_the_read_hash() {
    let mut e = loaded("fn a() {}\n", None, 1.0);
    assert_eq!(e.key(PATH, &key(Key::End, Mods::default())), Outcome::Handled);
    e.key(PATH, &key(Key::Text("!".into()), Mods::default()));
    assert_eq!(text(&e), "fn a() {}!\n");
    assert_eq!(e.key(PATH, &ctrl('s')), Outcome::Handled);
    assert_eq!(
        e.effects,
        [Effect::Write { path: PATH.into(), content: "fn a() {}!\n".into(), expected: Some(sha256_hex("fn a() {}\n")) }]
    );
}

#[test]
fn ctrl_f_seeds_find_with_the_selection_and_enter_steps_through_matches() {
    let mut e = loaded("total total total", None, 1.0);
    e.key(PATH, &key(Key::Right, Mods { shift: true, ctrl: true, ..Mods::default() }));
    e.key(PATH, &ctrl('f'));
    let bar = e.panes[PATH].find.as_ref().expect("find opens");
    assert_eq!(bar.query.text, "total");
    e.find_step(PATH, true);
    assert_eq!(e.docs.docs[PATH].editor.selection().range(), 6..11);
    e.key(PATH, &key(Key::Escape, Mods::default()));
    assert!(e.panes[PATH].find.is_none());
}

#[test]
fn copy_and_paste_go_through_the_window() {
    let mut e = loaded("hello", None, 1.0);
    e.key(PATH, &ctrl('a'));
    assert_eq!(e.key(PATH, &ctrl('c')), Outcome::Copy("hello".into()));
    assert_eq!(e.key(PATH, &ctrl('v')), Outcome::Paste);
    assert_eq!(e.key(PATH, &ctrl('x')), Outcome::Copy("hello".into()));
    assert_eq!(text(&e), "");
}

#[test]
fn a_draft_conflict_locks_the_buffer() {
    let mut e = loaded("disk", Some(Draft { content: "draft".into(), saved_at: 1.0 }), 0.0);
    assert_eq!(text(&e), "draft", "a draft newer than the disk applies");
    let mut e2 = Editors::default();
    e2.open(PATH, Some(Draft { content: "draft".into(), saved_at: 1.0 }));
    e2.step(Event::Loaded { path: PATH.into(), result: Ok(Read { content: "disk".into(), modified_at: 9.0, content_hash: None }) });
    e2.key(PATH, &key(Key::Text("x".into()), Mods::default()));
    assert_eq!(e2.docs.docs[PATH].editor.text(), "disk");
    e = e2;
    e.step(Event::ResolveDraft { path: PATH.into(), keep_draft: true });
    e.key(PATH, &key(Key::Text("x".into()), Mods::default()));
    assert_eq!(text(&e), "xdraft");
}

#[test]
fn an_input_method_commit_types_into_the_focused_editor() {
    let mut e = loaded("", None, 1.0);
    assert!(!e.ime(&Ime::Commit("日本".into())), "no editor has the keyboard");
    e.focused = Some(PATH.into());
    e.ime(&Ime::Preedit("にほん".into(), None, Vec::new()));
    assert_eq!(e.panes[PATH].preedit.as_deref(), Some("にほん"));
    e.ime(&Ime::Commit("日本".into()));
    assert_eq!((text(&e).as_str(), e.panes[PATH].preedit.as_deref()), ("日本", None));
}

#[test]
fn go_to_line_moves_the_caret_and_closes() {
    let mut e = loaded("a\nb\nc\n", None, 1.0);
    e.key(PATH, &ctrl('g'));
    e.field(PATH, super::Field::GoTo).text = "3".into();
    assert!(e.go_to_line(PATH));
    assert_eq!(e.docs.docs[PATH].editor.line_col(), (3, 1));
    assert!(e.panes[PATH].goto.is_none());
}

#[test]
fn ctrl_h_in_the_find_field_opens_replace_and_moves_there() {
    let mut e = loaded("total total", None, 1.0);
    assert_eq!(e.key(PATH, &ctrl('f')), Outcome::Focus(super::Field::Query));
    assert_eq!(e.field_chord(PATH, &ctrl('h')), Outcome::Focus(super::Field::Replace));
    assert!(e.panes[PATH].find.as_ref().expect("find stays open").replacing);
    assert_eq!(e.field_chord(PATH, &key(Key::Text("x".into()), Mods::default())), Outcome::Ignored);
}

#[test]
fn find_as_you_type_selects_the_first_match_after_the_caret() {
    let mut e = loaded("pub fn total() {}\nfn totals() {}\n// end", None, 1.0);
    e.key(PATH, &key(Key::End, Mods { ctrl: true, ..Mods::default() }));
    e.key(PATH, &ctrl('f'));
    for typed in ["t", "to", "tot", "tota", "total"] {
        *e.field(PATH, super::Field::Query) = rune::TextEdit::new(typed.to_owned());
        e.follow_find();
    }
    let sel = e.docs.docs[PATH].editor.selection();
    assert_eq!((sel.anchor, sel.head), (7, 12));
}
