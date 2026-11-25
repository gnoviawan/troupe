use model::editor::{Event, Read, sha256_hex};
use model::persistence::{Cursor, OpenFile, ViewMode};
use rune::{Key, KeyEvent, Mods};
use serde_json::Value;

use crate::State;
use crate::host::Fetch;

const PATH: &str = "/w/alpha/src/main.rs";

fn typed(s: &mut State, text: &str) {
    s.editors.key(PATH, &KeyEvent { key: Key::End, mods: Mods { ctrl: true, cmd: true, ..Mods::default() } });
    s.editors.key(PATH, &KeyEvent { key: Key::Text(text.into()), mods: Mods::default() });
}

/// A state with `main.rs` open and loaded as "a", and its record in the workspace files.
fn opened() -> State {
    let mut s = State::default();
    s.workspace.files.open.push(OpenFile {
        file_path: PATH.into(),
        cursor_position: Cursor { line: 1, col: 1 },
        scroll_top: 0.0,
        view_mode: ViewMode::Code,
        is_dirty: false,
        draft_content: None,
        last_modified: 0.0,
    });
    s.editors.open(PATH, None);
    let read = Read { content: "a".into(), modified_at: 1.0, content_hash: Some(sha256_hex("a")) };
    s.editors.step(Event::Loaded { path: PATH.into(), result: Ok(read) });
    s.editors.effects.clear();
    s
}

fn landed(s: &mut State, content: &str) {
    s.editor_fetched(Fetch::FileWrite { path: PATH.into(), content: content.into() }, Ok(Value::Null));
}

#[test]
fn the_dialogs_save_waits_for_the_write_in_flight_then_writes_the_later_edits_before_closing() {
    let mut s = opened();
    typed(&mut s, "b");
    s.editors.save(PATH);
    s.run_editor_effects();
    typed(&mut s, "c");
    s.save_and_close(PATH.into());
    landed(&mut s, "ab");
    assert!(s.editor_host.closing.contains(PATH), "the tab waits: abc is not on disk yet");
    assert_eq!(s.editors.docs.docs[PATH].saving.as_deref(), Some("abc"), "the later edits are being written");
    landed(&mut s, "abc");
    assert!(!s.editor_host.closing.contains(PATH));
    assert!(!s.editors.docs.docs[PATH].is_dirty());
}

#[test]
fn edits_made_just_before_the_project_changes_reach_the_record() {
    let mut s = opened();
    typed(&mut s, "b");
    s.sync_drafts(10.0);
    assert_eq!(s.workspace.files.get(PATH).and_then(|f| f.draft_content.clone()), None, "still inside the debounce");
    s.flush_drafts();
    assert_eq!(s.workspace.files.get(PATH).and_then(|f| f.draft_content.clone()).as_deref(), Some("ab"));
}

#[test]
fn the_dialogs_save_with_no_open_file_and_no_draft_writes_nothing() {
    let mut s = State::default();
    s.save_and_close(PATH.into());
    assert!(s.editors.effects.is_empty() && s.editors.docs.docs.is_empty());
    assert!(!s.editor_host.closing.contains(PATH));
}
