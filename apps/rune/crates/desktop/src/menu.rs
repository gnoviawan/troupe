//! The app menu: File, Edit, View, Window and Help, item for item as the Tauri app builds it
//! (`build_app_menu` in `src-tauri/src/lib.rs`, with its ids). macOS gets it in the native
//! menu bar (Rune's standard menus add Edit and Minimize); Linux and Windows draw it in the
//! window, above the shell. Toggle DevTools has no Rune equivalent and is gone.
//!
//! One table (`menus`) and one dispatch (`run`) serve both bars.

use rune::{El, Key, KeyEvent, Mods, Tx, el, row, text};
use theme::Tokens;
use troupe_app::State;
use ui::overlay::{Menu, MenuItem};
use ui::type_::sans;

use crate::paths::Os;
use crate::services;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Reload,
    ActualSize,
    ZoomIn,
    ZoomOut,
    ToggleFullScreen,
    Minimize,
    Maximize,
    CloseWindow,
    CloseTab,
    CheckForUpdates,
    RevealLogs,
    ExportLog,
    CopyLog,
    ExportLogDefault,
    LearnMore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// The Tauri menu id, the label, and what it does.
    Item(&'static str, &'static str, Action),
    Separator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submenu {
    pub title: &'static str,
    pub entries: Vec<Entry>,
}

pub const LEARN_MORE_URL: &str = "https://github.com/gnoviawan/troupe";

/// The menus on `os`, in the Tauri app's order. macOS has no File menu (Quit lives in the
/// app menu) and its Window menu closes the tab with ⌘W instead of the window.
pub fn menus(os: Os) -> Vec<Submenu> {
    use Action as A;
    use Entry::{Item, Separator};
    let mut menus = Vec::new();
    if os != Os::Mac {
        menus.push(Submenu { title: "File", entries: vec![Item("quit", "Quit", A::Quit)] });
    }
    menus.push(Submenu {
        title: "Edit",
        entries: vec![
            Item("undo", "Undo", A::Undo),
            Item("redo", "Redo", A::Redo),
            Separator,
            Item("cut", "Cut", A::Cut),
            Item("copy", "Copy", A::Copy),
            Item("paste", "Paste", A::Paste),
            Item("select-all", "Select All", A::SelectAll),
        ],
    });
    menus.push(Submenu {
        title: "View",
        entries: vec![
            Item("view-reload", "Reload", A::Reload),
            Separator,
            Item("view-zoom-reset", "Actual Size", A::ActualSize),
            Item("view-zoom-in", "Zoom In", A::ZoomIn),
            Item("view-zoom-out", "Zoom Out", A::ZoomOut),
            Separator,
            Item("view-toggle-fullscreen", "Toggle Full Screen", A::ToggleFullScreen),
        ],
    });
    let close = match os {
        Os::Mac => Item("window-close-tab", "Close Tab", A::CloseTab),
        _ => Item("close-window", "Close Window", A::CloseWindow),
    };
    menus.push(Submenu {
        title: "Window",
        entries: vec![Item("minimize", "Minimize", A::Minimize), Item("maximize", "Maximize", A::Maximize), Separator, close],
    });
    menus.push(Submenu {
        title: "Help",
        entries: vec![
            Item("check-for-updates", "Check for Updates...", A::CheckForUpdates),
            Separator,
            Item("help-reveal-logs", "Reveal Log File", A::RevealLogs),
            Item("help-export-log-file", "Export Log File...", A::ExportLog),
            Item("help-copy-log-contents", "Copy Log Contents", A::CopyLog),
            Item("help-export-log-default", "Export Log to Default Directory", A::ExportLogDefault),
            Item("help-learn-more", "Learn More", A::LearnMore),
        ],
    });
    menus
}

/// A shortcut the menu shows beside an item, as the Tauri menu's accelerators.
fn shortcut(action: Action) -> Option<&'static str> {
    match action {
        Action::Reload => Some("Ctrl+R"),
        Action::CheckForUpdates => Some("Ctrl+Shift+U"),
        Action::Undo => Some("Ctrl+Z"),
        Action::Redo => Some("Ctrl+Shift+Z"),
        Action::Cut => Some("Ctrl+X"),
        Action::Copy => Some("Ctrl+C"),
        Action::Paste => Some("Ctrl+V"),
        Action::SelectAll => Some("Ctrl+A"),
        _ => None,
    }
}

/// The primary modifier and `c`, as a field receives Ctrl+C (⌘C on macOS).
fn edit_key(c: char, shift: bool) -> KeyEvent {
    let mac = rune::KeyPlatform::current() == rune::KeyPlatform::Mac;
    KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, ctrl: !mac, shift, ..Mods::default() } }
}

/// Does what `action` names.
pub fn run(action: Action, s: &mut State, tx: &mut Tx) {
    log::info!("[menu] {action:?}");
    match action {
        Action::Quit => tx.quit(),
        Action::Undo => tx.send_key(edit_key('z', false)),
        Action::Redo => tx.send_key(edit_key('z', true)),
        Action::Cut => tx.send_key(edit_key('x', false)),
        Action::Copy => tx.send_key(edit_key('c', false)),
        Action::Paste => tx.send_key(edit_key('v', false)),
        Action::SelectAll => tx.send_key(edit_key('a', false)),
        Action::Reload => troupe_app::reload(s),
        Action::ActualSize => s.fire("zoomReset", tx),
        Action::ZoomIn => s.fire("zoomIn", tx),
        Action::ZoomOut => s.fire("zoomOut", tx),
        Action::ToggleFullScreen => tx.toggle_full_screen(),
        Action::Minimize => tx.minimize(),
        Action::Maximize => tx.toggle_maximized(),
        Action::CloseWindow => tx.close_window(),
        Action::CloseTab => s.fire("closeTab", tx),
        Action::CheckForUpdates => s.toast_later("Check for Updates"),
        Action::RevealLogs => services::reveal_logs(s),
        Action::ExportLog => services::export_log(tx),
        Action::CopyLog => services::copy_log(s, tx),
        Action::ExportLogDefault => services::export_log_default(s),
        Action::LearnMore => {
            if let Err(e) = tx.open_url(LEARN_MORE_URL) {
                s.toast_error("Could not open the link", &e.to_string());
            }
        }
    }
}

/// The overlay key of a submenu of the in-window bar.
fn key(title: &str) -> String {
    format!("app-menu-{}", title.to_ascii_lowercase())
}

/// The in-window menu bar (Linux, Windows): a 28 px row of the five menus over the shell. A
/// submenu opens on a press; with one open, hovering another title opens that one instead.
pub fn bar(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let mut bar = row().key("app-menu-bar").role("menubar").w_full().h(27.0).px(4.0).items_center();
    let any_open = menus(Os::CURRENT).iter().any(|m| s.overlays.is_open(&key(m.title)));
    for menu in menus(Os::CURRENT) {
        let overlay = key(menu.title);
        let open = s.overlays.is_open(&overlay);
        let title = text(menu.title).font(sans(13.0, 400.0, 18.0)).color(c.foreground);
        let toggle = overlay.clone();
        let hover = overlay.clone();
        let trigger = row()
            .key(format!("{overlay}-title"))
            .role("menuitem")
            .label(menu.title)
            .h(24.0)
            .px(8.0)
            .items_center()
            .radius(4.0)
            .fill(if open { c.sidebar_accent } else { rune::Color::TRANSPARENT })
            .hover(c.sidebar_accent)
            .child(title)
            .on_press(move |s: &mut State| {
                let was_open = s.overlays.is_open(&toggle);
                close_all(s);
                if !was_open {
                    s.overlays.open(toggle.clone());
                }
            })
            .on_hover_change(move |s: &mut State, inside: bool, _: &mut Tx| {
                if inside && any_open && !s.overlays.is_open(&hover) {
                    close_all(s);
                    s.overlays.open(hover.clone());
                }
            });
        let mut dropdown = Menu::new(overlay.clone()).name(menu.title).open(open).width(240.0);
        for entry in menu.entries {
            dropdown = match entry {
                Entry::Separator => dropdown.separator(),
                Entry::Item(_, label, action) => {
                    let mut item = MenuItem::new(label).on_select_tx(move |s: &mut State, tx: &mut Tx| {
                        close_all(s);
                        run(action, s, tx);
                    });
                    if let Some(keys) = shortcut(action) {
                        item = item.shortcut(keys);
                    }
                    dropdown.item(item)
                }
            };
        }
        let close = overlay.clone();
        bar = bar.child(dropdown.on_close(move |s: &mut State| {
            s.overlays.close(&close);
        }).build(t, layer, trigger));
    }
    rune::col()
        .w_full()
        .fill(c.sidebar_background)
        .child(bar.child(el().grow(1.0).h_full().window_drag()))
        .child(el().w_full().h(1.0).fill(c.border))
}

fn close_all(s: &mut State) {
    for menu in menus(Os::CURRENT) {
        s.overlays.close(&key(menu.title));
    }
}

/// The native menu bar's items (macOS): the View, Window and Help items the standard menus
/// lack. Every item runs through `run`.
pub fn native_items() -> Vec<rune::MenuItem<State>> {
    menus(Os::Mac)
        .into_iter()
        .filter(|m| m.title != "Edit")
        .flat_map(|m| {
            m.entries.into_iter().filter_map(move |entry| match entry {
                Entry::Item(id, label, action) if !matches!(action, Action::Minimize) => {
                    Some(rune::MenuItem::new(label, native_action(action)).id(id).menu(m.title))
                }
                _ => None,
            })
        })
        .collect()
}

/// A plain `fn` for each action, as native items take.
fn native_action(action: Action) -> fn(&mut State, &mut Tx) {
    use Action as A;
    match action {
        A::Quit => |s, tx| run(A::Quit, s, tx),
        A::Undo => |s, tx| run(A::Undo, s, tx),
        A::Redo => |s, tx| run(A::Redo, s, tx),
        A::Cut => |s, tx| run(A::Cut, s, tx),
        A::Copy => |s, tx| run(A::Copy, s, tx),
        A::Paste => |s, tx| run(A::Paste, s, tx),
        A::SelectAll => |s, tx| run(A::SelectAll, s, tx),
        A::Reload => |s, tx| run(A::Reload, s, tx),
        A::ActualSize => |s, tx| run(A::ActualSize, s, tx),
        A::ZoomIn => |s, tx| run(A::ZoomIn, s, tx),
        A::ZoomOut => |s, tx| run(A::ZoomOut, s, tx),
        A::ToggleFullScreen => |s, tx| run(A::ToggleFullScreen, s, tx),
        A::Minimize => |s, tx| run(A::Minimize, s, tx),
        A::Maximize => |s, tx| run(A::Maximize, s, tx),
        A::CloseWindow => |s, tx| run(A::CloseWindow, s, tx),
        A::CloseTab => |s, tx| run(A::CloseTab, s, tx),
        A::CheckForUpdates => |s, tx| run(A::CheckForUpdates, s, tx),
        A::RevealLogs => |s, tx| run(A::RevealLogs, s, tx),
        A::ExportLog => |s, tx| run(A::ExportLog, s, tx),
        A::CopyLog => |s, tx| run(A::CopyLog, s, tx),
        A::ExportLogDefault => |s, tx| run(A::ExportLogDefault, s, tx),
        A::LearnMore => |s, tx| run(A::LearnMore, s, tx),
    }
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
