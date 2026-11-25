use model::explorer::{Effect, Entry, Event};

use super::Explorer;

fn listed() -> Explorer {
    let mut x = Explorer::default();
    x.step(Event::Root(Some("/p".into())));
    let entries = vec![
        Entry { name: "src".into(), path: "/p/src".into(), dir: true, ignored: false },
        Entry { name: "main.rs".into(), path: "/p/main.rs".into(), dir: false, ignored: false },
    ];
    x.step(Event::Listed { dir: "/p".into(), result: Ok(entries) });
    x.effects.clear();
    x
}

#[test]
fn rename_selects_the_name_up_to_its_extension() {
    let mut x = listed();
    x.step(Event::StartRename("/p/main.rs".into()));
    assert_eq!(x.name.text, "main.rs");
    assert_eq!(x.name.selected(), "main");
    x.step(Event::StartRename("/p/src".into()));
    assert_eq!((x.name.text.as_str(), x.name.selected()), ("src", "src"), "a folder name selects whole");
}

#[test]
fn submit_sends_the_typed_name_and_reveals_are_taken_apart() {
    let mut x = listed();
    x.step(Event::StartCreate { parent: Some("/p".into()), kind: model::explorer::Kind::File });
    assert_eq!(x.name.text, "");
    x.name = rune::TextEdit::new("lib.rs");
    x.submit();
    let op = model::explorer::Op::Create { path: "/p/lib.rs".into(), kind: model::explorer::Kind::File };
    assert_eq!(x.effects, [Effect::Run(op.clone())]);
    x.effects.clear();
    x.step(Event::Done { op, result: Ok(()) });
    assert_eq!(x.take_reveals(), ["/p/lib.rs"]);
    assert_eq!(x.effects, [Effect::List("/p".into())]);
}
