use client::ws::Command;
use rune::{Runtime, TextEdit};
use serde_json::{Value, json};
use troupe_protocol::ws::WsRequestType;

use super::{LEAVE, PROJECT};
use crate::State;
use crate::tests::{present, runtime_at, tap};

fn desktop() -> Runtime<State> {
    runtime_at((1280.0, 800.0))
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(0.016);
    rt.frame(0.032);
}

fn edit(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update_if(move |s, _| {
        f(s);
        true
    });
    frame(rt);
}

/// The `updateProject` payloads sent, oldest first.
fn updates(rt: &Runtime<State>) -> Vec<Value> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { kind: WsRequestType::UpdateProject, payload, .. } => Some(payload.clone()),
            _ => None,
        })
        .collect()
}

fn beta(rt: &Runtime<State>) -> &model::projects::Project {
    rt.state.projects.store.projects.iter().find(|p| p.id == "p-b").unwrap()
}

#[test]
fn the_rail_menu_opens_the_window_for_the_project_it_was_opened_on() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-b"), None));
    assert_eq!(rt.state.overlays.top(), Some(PROJECT));
    assert!(present(&rt, "settings-page-general"));
    assert_eq!(rt.state.project_ui.name.text, "beta");
    assert_eq!(rt.state.project_ui.path.text, "/w/beta");
    assert!(!present(&rt, "settings-footer"), "no footer before an edit");
}

#[test]
fn an_edit_shows_the_footer_and_save_sends_one_update_with_the_draft() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-b"), None));
    edit(&mut rt, |s| s.project_ui.name = TextEdit::new("beta 2"));
    tap(&mut rt, "swatch-red");
    assert!(present(&rt, "settings-footer"));
    tap(&mut rt, "page-env-vars");
    tap(&mut rt, "add-env");
    edit(&mut rt, |s| {
        s.project_ui.env[0].key = TextEdit::new(" API ");
        s.project_ui.env[0].value = TextEdit::new("x");
    });
    tap(&mut rt, "add-env");
    tap(&mut rt, "save");
    assert_eq!(updates(&rt), vec![json!({ "projectId": "p-b", "name": "beta 2", "color": "red" })]);
    let p = beta(&rt);
    assert_eq!(p.client.env_vars.as_ref().map(|v| (v.len(), v[0].key.as_str())), Some((1, "API")), "the blank key is dropped, the other trimmed");
    assert_eq!(p.client.default_shell.as_deref(), Some(""));
    assert!(!present(&rt, "settings-footer"), "saved");
    assert_eq!(rt.state.project_ui.env.len(), 1, "the form shows the normalized list");
}

#[test]
fn closing_a_dirty_draft_asks_and_leave_drops_the_edits() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(None, Some("symlinks")));
    tap(&mut rt, "add-dir");
    tap(&mut rt, "settings-back");
    assert!(rt.state.overlays.is_open(LEAVE), "the unsaved-changes confirm");
    assert!(rt.state.overlays.is_open(PROJECT));
    edit(&mut rt, |s| s.close(LEAVE));
    edit(&mut rt, |s| {
        s.close_top();
    });
    assert!(rt.state.overlays.is_open(LEAVE), "Escape (and the phone's Back) asks too");
    edit(&mut rt, |s| s.leave_project_settings());
    assert!(!rt.state.overlays.is_open(PROJECT));
    assert!(updates(&rt).is_empty());
    edit(&mut rt, |s| s.open_project_settings(None, Some("symlinks")));
    assert!(rt.state.project_ui.dirs.is_empty(), "the next open shows the saved list");
}

#[test]
fn discard_puts_the_saved_fields_back() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-a"), None));
    edit(&mut rt, |s| s.project_ui.name = TextEdit::new("renamed"));
    tap(&mut rt, "discard");
    assert_eq!(rt.state.project_ui.name.text, "alpha");
    assert!(!present(&rt, "settings-footer"));
    tap(&mut rt, "settings-back");
    assert!(!rt.state.overlays.is_open(PROJECT), "a clean window closes at once");
}

#[test]
fn removing_a_variable_removes_exactly_that_row() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-a"), Some("env-vars")));
    for key in ["A", "B", "C"] {
        tap(&mut rt, "add-env");
        let at = rt.state.project_ui.env.len() - 1;
        edit(&mut rt, move |s| s.project_ui.env[at].key = TextEdit::new(key));
    }
    let (x, y) = rt.find("env-1/remove").expect("B's remove button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(&mut rt);
    let keys: Vec<&str> = rt.state.project_ui.env.iter().map(|r| r.key.text.as_str()).collect();
    assert_eq!(keys, ["A", "C"]);
}

#[test]
fn an_env_import_merges_and_a_stale_read_is_dropped() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-a"), Some("env-vars")));
    edit(&mut rt, |s| s.import_env());
    let first = rt.state.project_ui.import_request;
    edit(&mut rt, |s| s.import_env());
    let second = rt.state.project_ui.import_request;
    let tag = |request| (request, "p-a".to_owned(), "/w/alpha".to_owned());
    edit(&mut rt, |s| s.env_read(tag(first), Ok(json!({ "content": "OLD=1" }))));
    assert!(rt.state.project_ui.env.is_empty(), "the first read was overtaken");
    edit(&mut rt, |s| s.env_read(tag(second), Ok(json!({ "content": "A=1\nbad\nB=\"two\"" }))));
    let rows: Vec<(&str, &str)> = rt.state.project_ui.env.iter().map(|r| (r.key.text.as_str(), r.value.text.as_str())).collect();
    assert_eq!(rows, [("A", "1"), ("B", "two")]);
    assert_eq!(rt.state.project_ui.import_warning.as_deref(), Some("Imported 2 variables.\nSkipped 1 invalid line(s):\nLine 2: bad"));
    assert!(present(&rt, "import-warning"));
    assert!(present(&rt, "settings-footer"));
}

#[test]
fn an_import_with_no_root_says_so_without_a_read() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-a"), Some("env-vars")));
    edit(&mut rt, |s| s.project_ui.path = TextEdit::new("  "));
    tap(&mut rt, "import-env");
    assert_eq!(rt.state.project_ui.import_error.as_deref(), Some("Project root is required to import .env."));
    assert!(present(&rt, "import-error"));
}

#[test]
fn sync_adds_only_the_new_existing_directories() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-a"), Some("symlinks")));
    edit(&mut rt, |s| {
        s.add_symlink_dir();
        s.project_ui.dirs[0] = TextEdit::new("node_modules");
        s.add_symlink_dir();
    });
    let reply = json!([
        { "dirName": "node_modules", "exists": true },
        { "dirName": ".venv", "exists": true },
        { "dirName": "dist", "exists": false }
    ]);
    edit(&mut rt, move |s| s.gitignore_parsed("p-a", super::GitignoreRead::Sync, Ok(reply)));
    let dirs: Vec<&str> = rt.state.project_ui.dirs.iter().map(|d| d.text.as_str()).collect();
    assert_eq!(dirs, ["node_modules", ".venv"], "the blank row goes, as React's merge drops it");
}

#[test]
fn a_removed_project_closes_its_window() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-c"), None));
    edit(&mut rt, |s| s.project_ui.name = TextEdit::new("dirty"));
    edit(&mut rt, |s| {
        let kept = s.projects.store.projects.iter().filter(|p| p.id != "p-c").cloned().collect();
        s.projects_step(model::projects::Event::SetProjects(kept, None, None));
    });
    assert!(!rt.state.overlays.is_open(PROJECT));
    assert!(rt.state.toasts.visible().iter().any(|t| t.title == "Project was removed, settings closed."));
}

#[test]
fn the_phone_window_stacks_its_rows_and_titles_the_project() {
    let mut rt = runtime_at((390.0, 844.0));
    edit(&mut rt, |s| s.open_project_settings(Some("p-b"), None));
    assert!(present(&rt, "settings-phone-bar"));
    assert!(present(&rt, "settings-close"));
    let (_, name_y) = rt.find("project-name-row/control/project-name").expect("the name field");
    let (_, label_y) = rt.find("project-name-row/words").expect("the label");
    assert!(name_y > label_y + 10.0, "the field sits under its label: {label_y} {name_y}");
}


#[test]
fn a_switch_of_the_active_project_reloads_the_form_so_save_never_writes_one_into_another() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(None, None));
    edit(&mut rt, |s| s.project_ui.name = TextEdit::new("alpha renamed"));
    edit(&mut rt, |s| s.select_project("p-b"));
    assert_eq!(rt.state.project_ui.inited.as_deref(), Some("p-b"));
    assert_eq!(rt.state.project_ui.name.text, "beta", "the form follows the active project");
    assert!(!rt.state.project_dirty(), "alpha's edit is not carried over");
    edit(&mut rt, |s| s.project_ui.name = TextEdit::new("beta 2"));
    tap(&mut rt, "save");
    assert_eq!(updates(&rt), vec![json!({ "projectId": "p-b", "name": "beta 2" })]);
}

#[test]
fn save_goes_to_the_project_the_form_was_loaded_from() {
    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-c"), None));
    edit(&mut rt, |s| s.project_ui.name = TextEdit::new("gamma 2"));
    edit(&mut rt, |s| s.select_project("p-b"));
    edit(&mut rt, |s| s.save_project_settings());
    assert_eq!(updates(&rt), vec![json!({ "projectId": "p-c", "name": "gamma 2" })]);
}

/// The rect of the live element keyed `key`.
fn rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

#[test]
fn colour_swatches_take_a_44px_touch_target_on_the_phone_and_stay_24px_on_the_desktop() {
    let mut rt = runtime_at((390.0, 844.0));
    edit(&mut rt, |s| s.open_project_settings(Some("p-b"), None));
    let [x1, _, w, h] = rect(&rt, "swatch-red");
    assert_eq!((w, h), (44.0, 44.0), "the touch hit area");
    let [x2, ..] = rect(&rt, "swatch-orange");
    assert_eq!(x2 - x1, 44.0, "hit areas meet without overlapping, so the dots stand 20 px apart");
    tap(&mut rt, "swatch-orange");
    assert_eq!(rt.state.project_ui.color, "orange");

    let mut rt = desktop();
    edit(&mut rt, |s| s.open_project_settings(Some("p-b"), None));
    let [x1, _, w, h] = rect(&rt, "swatch-red");
    let [x2, ..] = rect(&rt, "swatch-orange");
    assert_eq!((w, h, x2 - x1), (24.0, 24.0, 32.0), "desktop keeps 24 px dots 8 px apart");
}
