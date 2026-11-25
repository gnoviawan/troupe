use model::editor::{Event, Read, sha256_hex};
use model::explorer::{Entry, Event as Tree, Op};
use model::persistence::{Cursor, OpenFile, ViewMode};
use rune::{Key, KeyEvent, Mods};

use crate::State;

/// `files` open and loaded, each typed into so it holds unsaved edits, with their records.
fn editing(files: &[&str]) -> State {
    let mut s = State::default();
    for path in files {
        s.workspace.files.open.push(OpenFile {
            file_path: (*path).into(),
            cursor_position: Cursor { line: 1, col: 1 },
            scroll_top: 0.0,
            view_mode: ViewMode::Code,
            is_dirty: true,
            draft_content: Some("xdisk".into()),
            last_modified: 0.0,
        });
        s.editors.open(path, None);
        let read = Read { content: "disk".into(), modified_at: 1.0, content_hash: Some(sha256_hex("disk")) };
        s.editors.step(Event::Loaded { path: (*path).into(), result: Ok(read) });
        s.editors.key(path, &KeyEvent { key: Key::Text("x".into()), mods: Mods::default() });
    }
    s.editors.effects.clear();
    s.explorer.step(Tree::Root(Some("/w/alpha".into())));
    s
}

fn renamed(s: &mut State, from: &str, to: &str, dir: bool) {
    let entry = Entry { name: from.rsplit('/').next().unwrap().into(), path: from.into(), dir, ignored: false };
    s.explorer.tree.listings.insert("/w/alpha".into(), model::explorer::Listing::Ready(vec![entry.clone()]));
    s.explorer.step(Tree::StartRename(from.into()));
    s.explorer.step(Tree::Done { op: Op::Rename { from: from.into(), to: to.into() }, result: Ok(()) });
    s.run_explorer_effects();
}

fn buffer(s: &State, path: &str) -> Option<(String, bool)> {
    s.editors.docs.docs.get(path).map(|d| (d.editor.text(), d.is_dirty()))
}

#[test]
fn renaming_a_file_with_unsaved_edits_keeps_them_under_the_new_name() {
    let mut s = editing(&["/w/alpha/a.rs"]);
    renamed(&mut s, "/w/alpha/a.rs", "/w/alpha/b.rs", false);
    assert_eq!(buffer(&s, "/w/alpha/b.rs"), Some(("xdisk".into(), true)));
    assert_eq!(buffer(&s, "/w/alpha/a.rs"), None);
    let record = s.workspace.files.get("/w/alpha/b.rs").expect("the record follows");
    assert_eq!((record.is_dirty, record.draft_content.as_deref()), (true, Some("xdisk")));
    assert!(s.workspace.files.get("/w/alpha/a.rs").is_none());
}

#[test]
fn renaming_a_folder_moves_every_open_file_inside_it_with_its_edits() {
    let mut s = editing(&["/w/alpha/src/a.rs", "/w/alpha/src/deep/b.rs", "/w/alpha/srcx.rs"]);
    renamed(&mut s, "/w/alpha/src", "/w/alpha/lib", true);
    assert_eq!(buffer(&s, "/w/alpha/lib/a.rs"), Some(("xdisk".into(), true)));
    assert_eq!(buffer(&s, "/w/alpha/lib/deep/b.rs"), Some(("xdisk".into(), true)));
    assert_eq!(buffer(&s, "/w/alpha/srcx.rs"), Some(("xdisk".into(), true)), "a sibling sharing the prefix stays");
    assert!(s.workspace.files.get("/w/alpha/lib/deep/b.rs").is_some());
}
