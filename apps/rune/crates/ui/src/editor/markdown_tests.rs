//! The Markdown editor keeps a file's bytes: 20 fixture files (repository docs and edge cases:
//! CRLF, a byte order mark, no final newline, trailing spaces and tabs, mermaid, setext
//! headings, raw HTML, nested lists, unclosed fences, an empty file) are opened in the document
//! view, walked block by block with the caret so every block is drawn as source and rendered
//! again, and saved. Each save must write nothing, and the buffer as the disk would hold it must
//! equal the input byte for byte. Adding a heading must write exactly that line in front.

use std::path::{Path, PathBuf};

use model::editor::{Effect, Event, Read, sha256_hex};
use rune::{El, Env, Key, KeyEvent, Mods, Runtime};

use super::Editors;

fn fixtures() -> Vec<(PathBuf, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/editor/fixtures/markdown");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixture folder")
        .map(|e| e.expect("fixture entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .map(|p| {
            let bytes = std::fs::read(&p).expect("fixture bytes");
            (p, bytes)
        })
        .collect();
    files.sort();
    files
}

fn view(s: &Editors, _cx: &mut rune::Cx<Editors>) -> El {
    let t = theme::tokens("termul", true);
    let path = s.docs.docs.keys().next().cloned().unwrap_or_default();
    super::view(s, |e| e, |e| e, t, &path).unwrap_or_else(rune::el)
}

/// `path` opened with `disk` read from the host, in a headless window.
fn open(path: &str, disk: &str) -> Runtime<Editors> {
    let mut e = Editors::default();
    e.open(path, None);
    let read = Read { content: disk.to_owned(), modified_at: 1.0, content_hash: Some(sha256_hex(disk)) };
    e.step(Event::Loaded { path: path.to_owned(), result: Ok(read) });
    e.effects.clear();
    e.focused = Some(path.to_owned());
    let mut rt = Runtime::new(e, view, (1000.0, 2400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    rt
}

fn key(rt: &mut Runtime<Editors>, path: &str, key: Key, mods: Mods) {
    rt.update(|e, _| {
        e.key(path, &KeyEvent { key, mods });
    });
    rt.frame(0.016);
}

fn ctrl(c: char) -> (Key, Mods) {
    (Key::Command(c), Mods { cmd: true, ..Mods::default() })
}

/// Whether a block of the document is drawn rendered.
fn rendered_blocks(rt: &Runtime<Editors>) -> usize {
    rt.elements().iter().filter(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with("markdown-block-"))).count()
}

#[test]
fn twenty_fixtures_open_walk_and_save_byte_for_byte() {
    let files = fixtures();
    assert_eq!(files.len(), 20, "the plan's 20 fixture files");
    for (file, bytes) in files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let disk = String::from_utf8(bytes.clone()).expect("fixtures are UTF-8");
        let path = format!("/p/docs/{name}");
        let mut rt = open(&path, &disk);
        assert!(rt.state.previewing(&path), "{name} opens as a document");
        let blocks = {
            let e = &rt.state;
            e.panes[&path].blocks(&e.docs.docs[&path]).len()
        };
        for i in 0..blocks {
            rt.update(|e, _| e.edit_block(&path, i));
            rt.frame(0.016);
        }
        // Every line, by the caret: each block in turn is drawn as source.
        let lines = rt.state.docs.docs[&path].editor.len_lines();
        key(&mut rt, &path, Key::Home, Mods { ctrl: true, ..Mods::default() });
        for _ in 0..lines {
            key(&mut rt, &path, Key::Down, Mods::default());
        }
        rt.update(|e, _| e.focused = None);
        rt.frame(0.016);
        assert_eq!(rendered_blocks(&rt), blocks, "{name}: every block draws rendered once the editor is left");
        let (k, m) = ctrl('s');
        key(&mut rt, &path, k, m);
        let doc = &rt.state.docs.docs[&path];
        assert_eq!(rt.state.effects, [Effect::Saved { path: path.clone(), clean: true }], "{name}: nothing to write");
        assert_eq!(doc.disk_text().as_bytes(), bytes.as_slice(), "{name}: the buffer is the file's bytes");
    }
}

#[test]
fn a_heading_added_in_the_document_is_the_only_change_written() {
    for (file, bytes) in fixtures() {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let disk = String::from_utf8(bytes.clone()).expect("fixtures are UTF-8");
        let path = format!("/p/docs/{name}");
        let mut rt = open(&path, &disk);
        rt.update(|e, _| e.edit_block(&path, 0));
        rt.frame(0.016);
        for c in "# Added".chars() {
            key(&mut rt, &path, Key::Text(c.to_string()), Mods::default());
        }
        key(&mut rt, &path, Key::Enter, Mods::default());
        let (k, m) = ctrl('s');
        key(&mut rt, &path, k, m);
        let crlf = rt.state.docs.docs[&path].crlf;
        let bom = if disk.starts_with('\u{feff}') { "\u{feff}" } else { "" };
        let rest = disk.strip_prefix('\u{feff}').unwrap_or(&disk);
        // The heading goes in front of the first block's first line (after front matter or
        // leading blank lines).
        let first = rt.state.panes[&path].blocks(&rt.state.docs.docs[&path]).first().map_or(0, |b| b.lines.start);
        let line_starts: Vec<usize> = std::iter::once(0).chain(rest.match_indices('\n').map(|(i, _)| i + 1)).collect();
        let at = line_starts.get(first).copied().unwrap_or(rest.len());
        let added = if crlf { "# Added\r\n" } else { "# Added\n" };
        let want = format!("{bom}{}{added}{}", &rest[..at], &rest[at..]);
        let wrote = rt.state.effects.iter().find_map(|fx| match fx {
            Effect::Write { content, expected, .. } => Some((content.clone(), expected.clone())),
            _ => None,
        });
        assert_eq!(wrote, Some((want, Some(sha256_hex(&disk)))), "{name}: only the heading line is new");
    }
}

#[test]
fn the_caret_block_draws_as_source_and_a_mermaid_fence_as_a_diagram_elsewhere() {
    let disk = "# Flow\n\n```mermaid\ngraph TD\n  A-->B\n```\n\nAfter.\n";
    let path = "/p/flow.md";
    let mut rt = open(path, disk);
    rt.update(|e, _| e.edit_block(path, 2));
    rt.frame(0.016);
    assert_eq!(rendered_blocks(&rt), 2, "the heading and the diagram render; the caret's paragraph is source");
    assert!(rt.find("markdown-source-2").is_some(), "the caret's block is drawn as source");
    rt.update(|e, _| e.set_preview(path, false));
    rt.frame(0.016);
    assert_eq!(rendered_blocks(&rt), 0, "Source shows the code editor");
    assert!(rt.find("editor-lines-/p/flow.md").is_some());
}

#[test]
fn only_markdown_files_have_the_document_view() {
    assert!(super::is_markdown("/p/README.md"));
    assert!(super::is_markdown("/p/NOTES.MD"));
    assert!(!super::is_markdown("/p/.md"));
    assert!(!super::is_markdown("/p/main.rs"));
    let mut e = Editors::default();
    e.open("/p/main.rs", None);
    e.set_preview("/p/main.rs", true);
    assert!(!e.previewing("/p/main.rs"));
}
