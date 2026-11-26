//! The overlay scenes open their first surface under `--scene`, the phone sheet pops one
//! sub-view per Escape, and every open surface reaches the accessibility tree with its role.

use gallery::{State, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn scene(id: &'static str) -> Runtime<State> {
    let mut state = State::default();
    state.show(id);
    let mut rt = Runtime::new(state, view, (1280.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn present(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn tap(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
}

fn escape(rt: &mut Runtime<State>) {
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.016);
}

fn roles(rt: &Runtime<State>) -> Vec<(String, String)> {
    fn walk(v: &serde_json::Value, out: &mut Vec<(String, String)>) {
        if let Some(role) = v.get("role").and_then(|r| r.as_str()) {
            let name = v.get("name").or_else(|| v.get("label")).and_then(|n| n.as_str()).unwrap_or("");
            // AccessKit spells roles in kebab case ("menu-item-check-box").
            out.push((role.replace('-', ""), name.to_owned()));
        }
        match v {
            serde_json::Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
            serde_json::Value::Object(o) => o.values().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&rt.accessibility_json(), &mut out);
    out
}

#[test]
fn each_overlay_scene_opens_its_first_surface() {
    for (id, key) in [
        ("dialog", "rename-dialog"),
        ("sheet", "sheet-right-dialog"),
        ("confirm-dialog", "confirm-danger-dialog"),
        ("menus", "view-menu-menu"),
        ("command", "command-dialog"),
        ("select", "branch-listbox"),
        ("floating", "find-branch-content"),
        ("toast", "toaster"),
    ] {
        assert!(present(&scene(id), key), "{id} shows {key}");
    }
}

#[test]
fn the_phone_sheet_pushes_sub_views_and_escape_pops_one_at_a_time() {
    let mut rt = scene("sheet");
    escape(&mut rt);
    tap(&mut rt, "phone-trigger");
    assert!(present(&rt, "phone-sheet"));
    assert!(!present(&rt, "back"), "the root view has no Back");
    tap(&mut rt, "row-Effort");
    assert_eq!(rt.state.overlays.above("phone"), ["phone/Effort".to_owned()]);
    assert!(present(&rt, "back"));
    tap(&mut rt, "row-Thinking budget");
    assert_eq!(rt.state.overlays.depth(), 3);
    escape(&mut rt);
    assert_eq!(rt.state.overlays.top(), Some("phone/Effort"), "Escape pops one level");
    tap(&mut rt, "back");
    assert_eq!(rt.state.overlays.top(), Some("phone"));
    escape(&mut rt);
    assert!(!present(&rt, "phone-sheet"));
}

#[test]
fn open_surfaces_reach_the_accessibility_tree_with_their_roles() {
    let dialog = roles(&scene("dialog"));
    assert!(dialog.contains(&("dialog".into(), "Rename session".into())), "{dialog:?}");
    let menu = roles(&scene("menus"));
    for want in [("menu", ""), ("menuitemcheckbox", "Word wrap"), ("menuitem", "New terminal")] {
        assert!(menu.iter().any(|(r, n)| r == want.0 && n.contains(want.1)), "{want:?} in {menu:?}");
    }
    let select = roles(&scene("select"));
    // The trigger is behind the open listbox, so the tree leaves it out, as `aria-modal` does.
    assert!(select.iter().any(|(r, n)| r == "listbox" && n == "Branch"), "{select:?}");
    assert!(!select.iter().any(|(r, _)| r == "combobox"), "{select:?}");
    assert!(select.iter().any(|(r, n)| r == "listboxoption" && n == "feature/canvas"), "{select:?}");
}
