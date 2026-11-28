use rune::{Cx, El, Env, Runtime, col};
use troupe_app::State;

use super::{Action, Entry, bar, menus, native_items};
use crate::paths::Os;

fn ids(os: Os) -> Vec<(&'static str, Vec<&'static str>)> {
    menus(os)
        .into_iter()
        .map(|m| {
            let ids = m
                .entries
                .iter()
                .map(|e| match e {
                    Entry::Item(id, ..) => *id,
                    Entry::Separator => "-",
                })
                .collect();
            (m.title, ids)
        })
        .collect()
}

#[test]
fn linux_and_windows_get_the_tauri_menus_without_devtools() {
    let help = vec![
        "check-for-updates",
        "-",
        "help-reveal-logs",
        "help-export-log-file",
        "help-copy-log-contents",
        "help-export-log-default",
        "help-learn-more",
    ];
    assert_eq!(
        ids(Os::Linux),
        vec![
            ("File", vec!["quit"]),
            ("Edit", vec!["undo", "redo", "-", "cut", "copy", "paste", "select-all"]),
            ("View", vec!["view-reload", "-", "view-zoom-reset", "view-zoom-in", "view-zoom-out", "-", "view-toggle-fullscreen"]),
            ("Window", vec!["minimize", "maximize", "-", "close-window"]),
            ("Help", help.clone()),
        ]
    );
    assert_eq!(ids(Os::Windows), ids(Os::Linux));
    let mac = ids(Os::Mac);
    assert_eq!(mac.iter().map(|(t, _)| *t).collect::<Vec<_>>(), ["Edit", "View", "Window", "Help"]);
    assert_eq!(mac[2].1, ["minimize", "maximize", "-", "window-close-tab"]);
}

#[test]
fn the_native_bar_adds_what_the_standard_menus_lack() {
    // Edit and Minimize come from Rune's standard macOS menus.
    assert_eq!(native_items().len(), 5 + 2 + 6, "View 5, Window 2, Help 6");
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let mut layer = Vec::new();
    let t = theme::tokens(theme::DEFAULT_FAMILY, true);
    let bar = bar(s, t, &mut layer);
    ui::overlay::root(col().w_full().h_full().child(bar), layer)
}

fn tap(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    rt.frame(0.032);
}

fn shown(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

/// Where `action` sits in this platform's View menu.
fn view_entry(action: Action) -> usize {
    let view = menus(Os::CURRENT).into_iter().find(|m| m.title == "View").unwrap();
    view.entries.iter().position(|e| matches!(e, Entry::Item(_, _, a) if *a == action)).unwrap()
}

#[test]
fn view_zoom_in_from_the_window_menu_bar_zooms_and_closes_the_menu() {
    let mut rt = Runtime::new(State::default(), view, (1200.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    // macOS draws no File menu (Quit lives in the app menu); the bar follows this platform.
    for menu in menus(Os::CURRENT) {
        let title = menu.title.to_ascii_lowercase();
        assert!(shown(&rt, &format!("app-menu-{title}-title")), "{title}");
    }
    tap(&mut rt, "app-menu-view-title");
    assert!(shown(&rt, "app-menu-view-menu"));
    let zoom_in = view_entry(Action::ZoomIn);
    tap(&mut rt, &format!("app-menu-view-item{zoom_in}"));
    assert!((rt.state.zoom.factor() - 1.1).abs() < 1e-6, "zoomed to {}", rt.state.zoom.factor());
    assert!(!shown(&rt, "app-menu-view-menu"), "the menu closed");
    tap(&mut rt, "app-menu-view-title");
    let reset = view_entry(Action::ActualSize);
    tap(&mut rt, &format!("app-menu-view-item{reset}"));
    assert!((rt.state.zoom.factor() - 1.0).abs() < 1e-6);
}

#[test]
fn a_second_press_on_an_open_title_closes_its_menu() {
    let mut rt = Runtime::new(State::default(), view, (1200.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    tap(&mut rt, "app-menu-help-title");
    assert!(shown(&rt, "app-menu-help-menu"));
    tap(&mut rt, "app-menu-help-title");
    assert!(!shown(&rt, "app-menu-help-menu"));
}
