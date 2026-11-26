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
    assert!(fx.contains(&Effect::Saved { path: PATH.into(), clean: true }));
    let doc = &docs.docs[PATH];
    assert!(!doc.is_dirty());
    assert_eq!(doc.hash.as_deref(), Some(sha256_hex(&content).as_str()));
}

#[test]
fn a_clean_file_saves_without_writing() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    assert_eq!(step(&mut docs, Event::Save { path: PATH.into() }, 0.0), [Effect::Saved { path: PATH.into(), clean: true }]);
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

fn written(docs: &mut Docs, content: &str) -> Vec<Effect> {
    step(docs, Event::Written { path: PATH.into(), content: content.into(), result: Ok(()) }, 9.0)
}

#[test]
fn a_save_asked_for_during_a_write_writes_the_newer_text_once_the_first_lands() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    type_text(&mut docs, "b");
    step(&mut docs, Event::Save { path: PATH.into() }, 0.0);
    type_text(&mut docs, "c");
    assert_eq!(step(&mut docs, Event::Save { path: PATH.into() }, 0.0), [], "queued behind the write in flight");
    let fx = written(&mut docs, "ab");
    assert_eq!(fx.last(), Some(&Effect::Write { path: PATH.into(), content: "abc".into(), expected: Some(sha256_hex("ab")) }));
    assert!(!fx.iter().any(|e| matches!(e, Effect::Saved { .. })), "nothing may close before abc is on disk");
    assert_eq!(written(&mut docs, "abc").last(), Some(&Effect::Saved { path: PATH.into(), clean: true }));
}

#[test]
fn a_write_that_lands_under_newer_edits_reports_the_buffer_still_dirty() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    type_text(&mut docs, "b");
    step(&mut docs, Event::Save { path: PATH.into() }, 0.0);
    type_text(&mut docs, "c");
    assert_eq!(written(&mut docs, "ab").last(), Some(&Effect::Saved { path: PATH.into(), clean: false }));
    assert!(docs.docs[PATH].is_dirty());
}

#[test]
fn a_lost_reply_reads_the_file_back_and_counts_the_write_if_it_landed() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    type_text(&mut docs, "b");
    step(&mut docs, Event::Save { path: PATH.into() }, 0.0);
    let lost = Err(("NETWORK_ERROR".to_owned(), "reset".to_owned()));
    let fx = step(&mut docs, Event::Written { path: PATH.into(), content: "ab".into(), result: lost }, 0.0);
    assert_eq!(fx, [Effect::ReadBack { path: PATH.into(), content: "ab".into() }]);
    assert_eq!(step(&mut docs, Event::Save { path: PATH.into() }, 0.0), [], "no second write while the read back runs");
    let fx = step(&mut docs, Event::ReadBack { path: PATH.into(), content: "ab".into(), disk: Some("ab".into()) }, 0.0);
    assert_eq!(fx.last(), Some(&Effect::Saved { path: PATH.into(), clean: true }), "the queued save finds nothing left");
    assert!(!docs.docs[PATH].external_conflict);
}

#[test]
fn a_lost_reply_whose_write_did_not_land_keeps_the_read_hash() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    type_text(&mut docs, "b");
    step(&mut docs, Event::Save { path: PATH.into() }, 0.0);
    let lost = Err(("OUTCOME_UNKNOWN".to_owned(), String::new()));
    step(&mut docs, Event::Written { path: PATH.into(), content: "ab".into(), result: lost }, 0.0);
    let fx = step(&mut docs, Event::ReadBack { path: PATH.into(), content: "ab".into(), disk: Some("a".into()) }, 0.0);
    assert!(fx.iter().any(|e| matches!(e, Effect::SaveFailed { .. })));
    let doc = &docs.docs[PATH];
    assert_eq!((doc.hash.as_deref(), doc.is_dirty(), doc.external_conflict), (Some(sha256_hex("a").as_str()), true, false));
}

#[test]
fn an_unchecked_write_with_a_lost_reply_asks_the_user_again() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    type_text(&mut docs, "b");
    step(&mut docs, Event::Overwrite { path: PATH.into() }, 0.0);
    docs.docs.get_mut(PATH).unwrap().hash = None;
    let lost = Err(("NETWORK_ERROR".to_owned(), String::new()));
    let fx = step(&mut docs, Event::Written { path: PATH.into(), content: "ab".into(), result: lost }, 0.0);
    assert!(fx.iter().any(|e| matches!(e, Effect::SaveFailed { .. })));
    assert!(docs.docs[PATH].external_conflict);
}

#[test]
fn a_save_asked_for_while_the_file_loads_writes_the_restored_draft_with_the_read_hash() {
    let mut docs = Docs::default();
    step(&mut docs, Event::Open { path: PATH.into(), draft: Some(Draft { content: "draft".into(), saved_at: 5.0 }) }, 0.0);
    assert_eq!(step(&mut docs, Event::Save { path: PATH.into() }, 0.0), []);
    let fx = step(&mut docs, Event::Loaded { path: PATH.into(), result: Ok(read("disk", 1.0)) }, 0.0);
    assert_eq!(fx.last(), Some(&Effect::Write { path: PATH.into(), content: "draft".into(), expected: Some(sha256_hex("disk")) }));
}

#[test]
fn a_rename_moves_the_unsaved_buffer_to_the_new_path() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a", 1.0));
    type_text(&mut docs, "b");
    step(&mut docs, Event::Rename { from: PATH.into(), to: "/p/src/lib.rs".into() }, 0.0);
    let doc = &docs.docs["/p/src/lib.rs"];
    assert_eq!((doc.path.as_str(), doc.editor.text().as_str(), doc.is_dirty()), ("/p/src/lib.rs", "ab", true));
    assert!(!docs.docs.contains_key(PATH));
}

#[test]
fn a_crlf_file_is_written_back_with_crlf_and_reads_clean_after() {
    let mut docs = Docs::default();
    let disk = "# A\r\nb\r\n";
    open(&mut docs, None, read(disk, 1.0));
    assert_eq!(docs.docs[PATH].disk_text(), disk, "an unedited file is the disk bytes");
    type_text(&mut docs, "c\n");
    let content = "# A\r\nb\r\nc\r\n".to_owned();
    let fx = step(&mut docs, Event::Save { path: PATH.into() }, 5.0);
    assert_eq!(fx, [Effect::Write { path: PATH.into(), content: content.clone(), expected: Some(sha256_hex(disk)) }]);
    let fx = step(&mut docs, Event::Written { path: PATH.into(), content: content.clone(), result: Ok(()) }, 9.0);
    assert!(fx.contains(&Effect::Saved { path: PATH.into(), clean: true }));
    assert!(!docs.docs[PATH].is_dirty());
    assert_eq!(docs.docs[PATH].hash.as_deref(), Some(sha256_hex(&content).as_str()));
}

#[test]
fn a_file_with_mixed_line_breaks_is_written_with_lf() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a\r\nb\n", 1.0));
    assert!(!docs.docs[PATH].crlf);
}

#[test]
fn a_typed_byte_order_mark_in_a_file_without_one_saves_clean() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("a\n", 1.0));
    let doc = docs.docs.get_mut(PATH).unwrap();
    doc.editor.apply(Command::Move { motion: troupe_editor::Motion::DocStart, extend: false }, 0.0);
    doc.editor.apply(Command::Insert("\u{feff}".into()), 0.0);
    let content = "\u{feff}a\n".to_owned();
    let fx = step(&mut docs, Event::Save { path: PATH.into() }, 5.0);
    assert_eq!(fx, [Effect::Write { path: PATH.into(), content: content.clone(), expected: Some(sha256_hex("a\n")) }]);
    step(&mut docs, Event::Written { path: PATH.into(), content, result: Ok(()) }, 9.0);
    assert!(!docs.docs[PATH].is_dirty(), "the typed U+FEFF is text, and the disk holds it");
}

#[test]
fn a_reload_changes_the_version_even_though_the_revision_restarts() {
    let mut docs = Docs::default();
    open(&mut docs, None, read("one\n", 1.0));
    let before = docs.docs[PATH].version();
    step(&mut docs, Event::Reload { path: PATH.into() }, 2.0);
    step(&mut docs, Event::Loaded { path: PATH.into(), result: Ok(read("two\n", 3.0)) }, 3.0);
    assert_eq!(docs.docs[PATH].editor.revision(), before.1);
    assert_ne!(docs.docs[PATH].version(), before);
}
