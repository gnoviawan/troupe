//! Replays `out/terminal/keys.json` (`tools/parity/vectors/command-keys.ts`): the command
//! editor's keymap on `model::terminal::line::Line`, against what CodeMirror did.

use model::terminal::line::{Action, Key, Line};
use serde_json::{Value, json};
use troupe_editor::{Command, Selection};

use crate::common::{diff, out_dir, read_json, source_hash};

fn key(name: &str) -> Key {
    match name {
        "Enter" => Key::Enter,
        "Shift-Enter" => Key::ShiftEnter,
        "ArrowUp" => Key::Up,
        "ArrowDown" => Key::Down,
        "Ctrl-a" => Key::LineStart,
        "Ctrl-e" => Key::LineEnd,
        "Ctrl-k" => Key::KillToEnd,
        "Ctrl-u" => Key::KillToStart,
        "Ctrl-w" => Key::KillWord,
        "Ctrl-c" => Key::Interrupt,
        "Ctrl-d" => Key::Eof,
        other => panic!("no key {other}"),
    }
}

/// A UTF-16 offset into `text` as a char offset, and back.
fn chars(text: &str, units: u64) -> usize {
    let mut seen = 0;
    text.chars().take_while(|c| {
        seen += c.len_utf16() as u64;
        seen <= units
    })
    .count()
}

fn units(text: &str, chars: usize) -> u64 {
    text.chars().take(chars).map(|c| c.len_utf16() as u64).sum()
}

#[test]
fn the_command_editor_keys_do_what_codemirror_did() {
    let recorded = read_json(&out_dir().join("terminal/keys.json"));
    for (path, hash) in recorded["sources"].as_object().expect("sources") {
        assert_eq!(source_hash(path), hash.as_str().unwrap_or_default(), "{path} changed after keys.json was recorded");
    }
    let mut failures = Vec::new();
    for case in recorded["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().unwrap_or("?");
        let history: Vec<String> = case["history"].as_array().into_iter().flatten().filter_map(|h| h.as_str().map(str::to_owned)).collect();
        let mut line = Line::new(case["doc"].as_str().unwrap_or_default());
        let (mut submitted, mut copied, mut eof) = (Vec::<String>::new(), Vec::<String>::new(), 0);
        let mut terminal_selection = String::new();
        for (i, step) in case["steps"].as_array().expect("steps").iter().enumerate() {
            let now = i as f64 * 10.0;
            let mut handled = Value::Null;
            if let Some(text) = step["type"].as_str() {
                line.apply(Command::Insert(text.to_owned()), now);
            } else if let Some(text) = step["paste"].as_str() {
                line.paste(text, now);
            } else if let Some([a, b]) = step["select"].as_array().map(Vec::as_slice) {
                let doc = line.text();
                let sel = Selection { anchor: chars(&doc, a.as_u64().unwrap_or(0)), head: chars(&doc, b.as_u64().unwrap_or(0)) };
                line.apply(Command::Select(sel), now);
            } else if let Some(text) = step["selection"].as_str() {
                terminal_selection = text.to_owned();
            } else {
                let action = line.key(key(step["key"].as_str().expect("key")), &history, &terminal_selection, now);
                handled = json!(action != Action::Ignored);
                match action {
                    Action::Submit(text) => submitted.push(text),
                    Action::Copy(text) => copied.push(text),
                    Action::SendEof => eof += 1,
                    Action::Ignored | Action::Handled => {}
                }
            }
            let doc = line.text();
            let sel = line.editor.selection();
            let got = json!({
                "handled": handled,
                "doc": doc,
                "anchor": units(&doc, sel.anchor),
                "head": units(&doc, sel.head),
                "submitted": submitted,
                "copied": copied,
                "eof": eof,
            });
            let mut want = step.clone();
            for k in ["type", "paste", "select", "selection", "key"] {
                want.as_object_mut().map(|m| m.remove(k));
            }
            if let Some(problem) = diff("", &want, &got) {
                failures.push(format!("{name} step {i} {}: {problem}", step["key"].as_str().unwrap_or("")));
            }
        }
    }
    assert!(failures.is_empty(), "{} steps differ:\n{}", failures.len(), failures.join("\n"));
}
