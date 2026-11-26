use serde_json::{Value, json};

use crate::State;
use crate::settings_profiles::EDITOR;

fn names(s: &State) -> Vec<String> {
    s.settings.get("terminalProfiles").as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_owned()).collect()
}

fn loaded() -> State {
    let mut s = State::default();
    s.settings_loaded(Ok(Some(json!({
        "terminalProfiles": [
            { "id": "a", "name": "Alpha", "shell": "", "args": [], "cwd": "", "env": {} },
            { "id": "b", "name": "Beta", "shell": "zsh", "args": ["-l"], "cwd": "/w", "env": { "A": "1" } },
            { "id": "c", "name": "Gamma", "shell": "", "args": [], "cwd": "", "env": {} }
        ]
    }))));
    s
}

#[test]
fn a_new_profile_needs_a_name_then_lands_at_the_end() {
    let mut s = loaded();
    s.edit_profile(None);
    assert!(s.overlays.is_open(EDITOR));
    s.save_profile();
    assert_eq!(s.settings_ui.profile.errors.name.as_deref(), Some("Name is required."));
    assert!(s.overlays.is_open(EDITOR));
    s.settings_ui.profile.name = rune::TextEdit::new("Node dev");
    s.settings_ui.profile.env = rune::TextEdit::new("NODE_ENV=development");
    s.save_profile();
    assert!(!s.overlays.is_open(EDITOR));
    assert_eq!(names(&s), ["Alpha", "Beta", "Gamma", "Node dev"]);
    assert_eq!(s.settings.get("terminalProfiles")[3]["env"], json!({ "NODE_ENV": "development" }));
}

#[test]
fn an_edit_keeps_the_profile_id_and_place() {
    let mut s = loaded();
    let beta = s.profiles().into_iter().find(|p| p.id == "b");
    s.edit_profile(beta.as_ref());
    assert_eq!(s.settings_ui.profile.args.text, "-l");
    assert!(s.settings_ui.profile.other, "zsh is not a detected shell, so its path shows");
    s.settings_ui.profile.name = rune::TextEdit::new("Beta 2");
    s.save_profile();
    assert_eq!(names(&s), ["Alpha", "Beta 2", "Gamma"]);
    assert_eq!(s.settings.get("terminalProfiles")[1]["id"], json!("b"));
}

#[test]
fn delete_removes_exactly_the_picked_profile_and_undo_puts_it_back() {
    let mut s = loaded();
    s.delete_profile("b");
    assert_eq!(names(&s), ["Alpha", "Gamma"]);
    let (toast, _) = s.settings_ui.undo.clone().expect("an undo toast");
    let shown = s.toasts.visible().iter().find(|t| t.id == toast).expect("on screen");
    assert_eq!((shown.title.as_str(), shown.action.as_deref(), shown.until), ("Deleted Beta", Some("Undo"), None));
    s.run_undo(toast);
    assert_eq!(names(&s), ["Alpha", "Beta", "Gamma"]);
    assert!(s.settings_ui.undo.is_none());
    assert!(s.toasts.visible().iter().all(|t| t.id != toast));
}

#[test]
fn a_second_delete_replaces_the_undo_toast() {
    let mut s = loaded();
    s.delete_profile("a");
    let (first, _) = s.settings_ui.undo.clone().unwrap();
    s.delete_profile("c");
    let (second, _) = s.settings_ui.undo.clone().unwrap();
    assert!(s.toasts.visible().iter().all(|t| t.id != first));
    s.run_undo(first);
    assert_eq!(names(&s), ["Beta"], "the replaced toast's undo does nothing");
    s.run_undo(second);
    assert_eq!(names(&s), ["Beta", "Gamma"]);
    assert_eq!(s.settings.get("terminalProfiles").as_array().map(Vec::len), Some(2));
    let _: &Value = s.settings.get("terminalProfiles");
}
