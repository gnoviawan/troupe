use super::{Docs, Draft, Effect, Event, Load, Read, Unavailable, sha256_hex, step};
use troupe_editor::Command;

const PATH: &str = "/p/src/main.rs";

fn read(content: &str, modified_at: f64) -> Read {
    Read { content: content.to_owned(), modified_at, content_hash: Some(sha256_hex(content)) }
}

fn open(docs: &mut Docs, draft: Option<Draft>, disk: Read) {
    let fx = step(docs, Event::Open { path: PATH.into(), draft }, 0.0);
    assert_eq!(fx, [Effect::Read { path: PATH.into() }]);
    step(docs, Event::Loaded { path: PATH.into(), result: Ok(disk) }, 0.0);
}

fn type_text(docs: &mut Docs, text: &str) {
    let doc = docs.docs.get_mut(PATH).unwrap();
    doc.editor.apply(Command::Move { motion: troupe_editor::Motion::DocEnd, extend: false }, 0.0);
    doc.editor.apply(Command::Insert(text.into()), 0.0);
}

#[test]
fn a_save_writes_with_the_hash_it_read_and_marks_the_file_clean() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("fn main() {}\n", 1.0));
    assert!(!docs.docs[PATH].is_dirty());
    type_text(&mut docs, "// x\n");
    assert!(docs.docs[PATH].is_dirty());
    let fx = step(&mut docs, Event::Save { path: PATH.into() }, 5.0);
    let content = "fn main() {}\n// x\n".to_owned();
    assert_eq!(
        fx,
        [Effect::Write { path: PATH.into(), content: content.clone(), expected: Some(sha256_hex("fn main() {}\n")) }]
    );
    assert!(step(&mut docs, Event::Save { path: PATH.into() }, 5.0).is_empty(), "one write at a time");
    let fx = step(&mut docs, Event::Written { path: PATH.into(), content: content.clone(), result: Ok(()) }, 9.0);
    assert!(fx.contains(&Effect::Saved { path: PATH.into() }));
    let doc = &docs.docs[PATH];
    assert!(!doc.is_dirty());
    assert_eq!(doc.hash.as_deref(), Some(sha256_hex(&content).as_str()));
}

#[test]
fn a_clean_file_saves_without_writing() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    assert_eq!(step(&mut docs, Event::Save { path: PATH.into() }, 0.0), [Effect::Saved { path: PATH.into() }]);
}

#[test]
fn a_refused_save_raises_the_external_change_and_overwrite_drops_the_precondition() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    type_text(&mut docs, "b");
    step(&mut docs, Event::Save { path: PATH.into() }, 0.0);
    let refused = Err(("PRECONDITION_FAILED".to_owned(), "changed".to_owned()));
    let fx = step(&mut docs, Event::Written { path: PATH.into(), content: "ab".into(), result: refused }, 0.0);
    assert!(fx.contains(&Effect::ExternalChange { path: PATH.into() }));
    assert!(docs.docs[PATH].external_conflict && docs.docs[PATH].is_dirty());
    let fx = step(&mut docs, Event::Overwrite { path: PATH.into() }, 0.0);
    assert_eq!(fx, [Effect::Write { path: PATH.into(), content: "ab".into(), expected: None }]);
}

#[test]
fn a_draft_not_older_than_the_disk_file_comes_back_as_an_unsaved_edit() {
    let mut docs = Docs::default();
    open(&mut docs, Some(Draft { content: "draft".into(), saved_at: 10.0 }), read("disk", 5.0));
    let doc = &docs.docs[PATH];
    assert_eq!(doc.editor.text(), "draft");
    assert!(doc.is_dirty() && doc.draft_conflict.is_none());
}

#[test]
fn a_draft_older_than_the_disk_file_shows_the_conflict_and_locks_the_buffer() {
    let mut docs = Docs::default();
    open(&mut docs, Some(Draft { content: "draft".into(), saved_at: 5.0 }), read("disk", 10.0));
    let doc = &docs.docs[PATH];
    assert_eq!(doc.editor.text(), "disk");
    assert!(doc.read_only() && doc.needs_attention() && !doc.is_dirty());
    assert_eq!(doc.draft().map(|d| d.content), Some("draft".to_owned()));
    assert_eq!(step(&mut docs, Event::Save { path: PATH.into() }, 0.0).len(), 1);
    step(&mut docs, Event::ResolveDraft { path: PATH.into(), keep_draft: true }, 0.0);
    let doc = &docs.docs[PATH];
    assert_eq!((doc.editor.text().as_str(), doc.is_dirty(), doc.read_only()), ("draft", true, false));
}

#[test]
fn using_the_disk_version_drops_the_draft() {
    let mut docs = Docs::default();
    open(&mut docs, Some(Draft { content: "draft".into(), saved_at: 5.0 }), read("disk", 10.0));
    step(&mut docs, Event::ResolveDraft { path: PATH.into(), keep_draft: false }, 0.0);
    let doc = &docs.docs[PATH];
    assert_eq!((doc.editor.text().as_str(), doc.needs_attention()), ("disk", false));
}

#[test]
fn a_draft_equal_to_the_disk_text_is_nothing() {
    let mut docs = Docs::default();
    open(&mut docs, Some(Draft { content: "same".into(), saved_at: 1.0 }), read("same", 10.0));
    assert!(!docs.docs[PATH].needs_attention());
}

#[test]
fn a_failed_read_shows_on_the_tab() {
    let mut docs = Docs::default();
    step(&mut docs, Event::Open { path: PATH.into(), draft: None }, 0.0);
    step(&mut docs, Event::Loaded { path: PATH.into(), result: Err(("NOT_FOUND".into(), "gone".into())) }, 0.0);
    assert_eq!(docs.docs[PATH].load, Load::Unavailable(Unavailable::Missing));
}

#[test]
fn a_file_over_the_read_limit_reads_as_too_large() {
    let u = Unavailable::from_refusal("FILE_TOO_LARGE", "file too large (1088890 bytes, max 1048576)");
    assert_eq!(u, Unavailable::TooLarge { size: Some(1_088_890), limit: Some(1_048_576) });
    assert_eq!(u.headline(), "This file is too large to preview");
    assert_eq!(u.detail().as_deref(), Some("1.0 MB, the preview limit is 1.0 MB"));
    assert_eq!(super::format_size(2048), "2.0 KB");
}

#[test]
fn the_hash_matches_the_hosts() {
    assert_eq!(sha256_hex("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}
