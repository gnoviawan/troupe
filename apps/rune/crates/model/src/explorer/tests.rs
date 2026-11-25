use serde_json::json;

use super::{Effect, Entry, Event, Explorer, Inline, Kind, Listing, Nav, Op, Row, parent_of, step};

fn file(path: &str) -> Entry {
    Entry { name: path.rsplit('/').next().unwrap().into(), path: path.into(), dir: false, ignored: false }
}

fn dir(path: &str) -> Entry {
    Entry { dir: true, ..file(path) }
}

/// `/p` listed as `src/` and `README.md`, `src` as `main.rs`.
fn tree() -> Explorer {
    let mut x = Explorer::default();
    assert_eq!(step(&mut x, Event::Root(Some("/p".into()))), [Effect::List("/p".into())]);
    step(&mut x, Event::Listed { dir: "/p".into(), result: Ok(vec![dir("/p/src"), file("/p/README.md")]) });
    x
}

fn names(x: &Explorer) -> Vec<String> {
    x.rows()
        .iter()
        .map(|r| match r {
            Row::Entry { entry, depth, .. } => format!("{}{}", "  ".repeat(*depth as usize), entry.name),
            Row::Create { depth, .. } => format!("{}<new>", "  ".repeat(*depth as usize)),
        })
        .collect()
}

#[test]
fn a_listing_parses_the_hosts_directory_entries() {
    let value = json!([
        { "name": "src", "path": "/p/src", "type": "directory", "extension": null, "size": 0, "modifiedAt": 1 },
        { "name": "a.rs", "path": "/p/a.rs", "type": "file", "extension": "rs", "size": 3, "modifiedAt": 1, "ignored": true }
    ]);
    let entries = Entry::parse_listing(&value).expect("an array");
    assert_eq!(entries, [dir("/p/src"), Entry { ignored: true, ..file("/p/a.rs") }]);
}

#[test]
fn pressing_a_folder_lists_and_opens_it_and_a_file_opens_in_a_tab() {
    let mut x = tree();
    assert_eq!(step(&mut x, Event::Press("/p/src".into())), [Effect::List("/p/src".into())]);
    assert!(matches!(x.rows()[0], Row::Entry { loading: true, open: true, .. }));
    step(&mut x, Event::Listed { dir: "/p/src".into(), result: Ok(vec![file("/p/src/main.rs")]) });
    assert_eq!(names(&x), ["src", "  main.rs", "README.md"]);
    assert_eq!(step(&mut x, Event::Press("/p/src/main.rs".into())), [Effect::Open("/p/src/main.rs".into())]);
    assert_eq!(step(&mut x, Event::Press("/p/src".into())), []);
    assert_eq!(names(&x), ["src", "README.md"], "a second press closes the folder, keeping its listing");
    assert_eq!(step(&mut x, Event::Press("/p/src".into())), []);
}

#[test]
fn arrows_walk_the_rows_and_left_and_right_close_and_open() {
    let mut x = tree();
    step(&mut x, Event::Nav(Nav::Down));
    assert_eq!(x.selected.as_deref(), Some("/p/src"));
    assert_eq!(step(&mut x, Event::Nav(Nav::Right)), [Effect::List("/p/src".into()), Effect::Reveal("/p/src".into())]);
    step(&mut x, Event::Listed { dir: "/p/src".into(), result: Ok(vec![file("/p/src/main.rs")]) });
    step(&mut x, Event::Nav(Nav::Right));
    assert_eq!(x.selected.as_deref(), Some("/p/src/main.rs"));
    step(&mut x, Event::Nav(Nav::Left));
    assert_eq!(x.selected.as_deref(), Some("/p/src"), "left on a file goes to its folder");
    step(&mut x, Event::Nav(Nav::Left));
    assert!(!x.open.contains("/p/src"));
    step(&mut x, Event::Nav(Nav::End));
    assert_eq!(x.selected.as_deref(), Some("/p/README.md"));
}

#[test]
fn new_file_from_the_header_goes_into_the_selected_files_folder_and_selects_it() {
    let mut x = tree();
    step(&mut x, Event::Press("/p/src".into()));
    step(&mut x, Event::Listed { dir: "/p/src".into(), result: Ok(vec![file("/p/src/main.rs")]) });
    step(&mut x, Event::Select("/p/src/main.rs".into()));
    step(&mut x, Event::StartCreate { parent: None, kind: Kind::File });
    assert_eq!(names(&x), ["src", "  <new>", "  main.rs", "README.md"]);
    let fx = step(&mut x, Event::Submit("  lib.rs ".into()));
    let op = Op::Create { path: "/p/src/lib.rs".into(), kind: Kind::File };
    assert_eq!(fx, [Effect::Run(op.clone())]);
    let fx = step(&mut x, Event::Done { op, result: Ok(()) });
    assert_eq!(fx, [Effect::List("/p/src".into()), Effect::Reveal("/p/src/lib.rs".into())]);
    assert_eq!((x.inline.clone(), x.selected.as_deref()), (None, Some("/p/src/lib.rs")));
}

#[test]
fn a_name_with_a_separator_is_refused_and_the_field_stays() {
    let mut x = tree();
    step(&mut x, Event::StartCreate { parent: Some("/p".into()), kind: Kind::Folder });
    let fx = step(&mut x, Event::Submit("a/b".into()));
    assert_eq!(fx, [Effect::Toast("Invalid name: file and folder names cannot contain path separators".into())]);
    assert_eq!(x.inline, Some(Inline::Create { parent: "/p".into(), kind: Kind::Folder }));
    assert_eq!(step(&mut x, Event::Submit("   ".into())), []);
    assert_eq!(x.inline, None, "an empty name cancels");
}

#[test]
fn a_rename_lands_follows_open_tabs_and_relists_the_folder() {
    let mut x = tree();
    step(&mut x, Event::StartRename("/p/README.md".into()));
    assert!(matches!(x.rows()[1], Row::Entry { renaming: true, .. }));
    let fx = step(&mut x, Event::Submit("NOTES.md".into()));
    let op = Op::Rename { from: "/p/README.md".into(), to: "/p/NOTES.md".into() };
    assert_eq!(fx, [Effect::Run(op.clone())]);
    let fx = step(&mut x, Event::Done { op, result: Ok(()) });
    assert_eq!(
        fx,
        [Effect::List("/p".into()), Effect::Renamed { from: "/p/README.md".into(), to: "/p/NOTES.md".into() }]
    );
    assert_eq!(x.selected.as_deref(), Some("/p/NOTES.md"));
}

#[test]
fn a_failed_rename_keeps_the_field_and_says_why() {
    let mut x = tree();
    step(&mut x, Event::StartRename("/p/README.md".into()));
    let op = Op::Rename { from: "/p/README.md".into(), to: "/p/src".into() };
    let fx = step(&mut x, Event::Done { op, result: Err("ALREADY_EXISTS: /p/src exists".into()) });
    assert_eq!(fx[0], Effect::Toast("ALREADY_EXISTS: /p/src exists".into()));
    assert!(x.inline.is_some());
}

#[test]
fn delete_asks_first_then_forgets_the_folder_and_closes_its_tabs() {
    let mut x = tree();
    step(&mut x, Event::Press("/p/src".into()));
    step(&mut x, Event::Listed { dir: "/p/src".into(), result: Ok(vec![file("/p/src/main.rs")]) });
    step(&mut x, Event::RequestDelete("/p/src".into()));
    let entry = x.deleting.clone().expect("the confirm opens");
    assert_eq!((super::delete_title(&entry), super::delete_message(&entry).as_str()), ("Delete folder", "Delete \"src\"? This cannot be undone."));
    let fx = step(&mut x, Event::ConfirmDelete);
    assert_eq!(fx, [Effect::Run(Op::Delete { entry: entry.clone() })]);
    assert_eq!(step(&mut x, Event::ConfirmDelete), [], "a second confirm while busy sends nothing");
    let fx = step(&mut x, Event::Done { op: Op::Delete { entry }, result: Ok(()) });
    assert_eq!(fx, [Effect::List("/p".into()), Effect::Deleted { path: "/p/src".into() }]);
    assert_eq!((x.deleting.clone(), x.busy), (None, false));
    assert!(!x.listings.contains_key("/p/src") && !x.open.contains("/p/src"));
    assert_eq!(x.listings.get("/p").map(|l| matches!(l, Listing::Ready(_))), Some(true));
}

#[test]
fn a_new_root_resets_the_tree_and_a_late_listing_of_the_old_one_is_dropped() {
    let mut x = tree();
    step(&mut x, Event::Root(Some("/q".into())));
    step(&mut x, Event::Listed { dir: "/p".into(), result: Ok(vec![file("/p/x")]) });
    assert!(!x.listings.contains_key("/p"));
    assert_eq!(x.listings.get("/q"), Some(&Listing::Loading));
}

#[test]
fn parent_of_keeps_the_root_slash() {
    assert_eq!((parent_of("/p/a"), parent_of("/a"), parent_of("C:\\p\\a")), ("/p", "/", "C:\\p"));
}

#[test]
fn duplicate_copies_beside_the_entry_with_copy_before_the_extension() {
    let mut x = tree();
    let fx = step(&mut x, Event::Duplicate("/p/README.md".into()));
    let op = Op::Copy { from: "/p/README.md".into(), to: "/p/README copy.md".into() };
    assert_eq!(fx, [Effect::Run(op.clone())]);
    assert_eq!(step(&mut x, Event::Done { op, result: Ok(()) }), [Effect::List("/p".into())]);
    let fx = step(&mut x, Event::Duplicate("/p/src".into()));
    assert_eq!(fx, [Effect::Run(Op::Copy { from: "/p/src".into(), to: "/p/src copy".into() })]);
}

#[test]
fn browsing_a_folder_lists_it_once_without_opening_it() {
    let mut x = tree();
    assert_eq!(step(&mut x, Event::Browse("/p/src".into())), [Effect::List("/p/src".into())]);
    assert_eq!(step(&mut x, Event::Browse("/p/src".into())), []);
    assert!(!x.open.contains("/p/src"));
}
