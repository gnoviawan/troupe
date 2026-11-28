//! The keyboard: the 25 default shortcuts (`use-keyboard-shortcuts.ts`) dispatched from keys no
//! control claimed, and the shortcut menu (`TitleBarShortcutsPopover`) whose rows rebind one by
//! recording the next chord (`ShortcutRecorder`). Bindings persist in the shared
//! `settings/keyboard-shortcuts` record, so a rebinding survives a reload and reaches the React
//! client too.

use model::shortcuts::{
    self, Event, KeyEvent as Chord, Platform, Shortcuts, find_conflicting_shortcut,
    format_key_for_display,
};
use rune::{El, Key, KeyEvent, Tx, col, icon, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use ui::type_::{mono, sans};

use crate::State;
use crate::sync::Shared;

pub const MENU: &str = "shortcut-menu";

/// The rows the shortcut menu lists, in its order (`QUICK_SHORTCUT_IDS`); `newBrowserTab` is
/// desktop-only.
const QUICK: [&str; 10] = [
    "commandPalette",
    "commandHistory",
    "newTerminal",
    "newBrowserTab",
    "sidebarToggle",
    "toggleFileExplorer",
    "settings",
    "zoomIn",
    "zoomOut",
    "zoomReset",
];

#[derive(Default)]
pub struct KeysUi {
    /// The shortcut whose row is recording the next chord.
    pub recording: Option<String>,
    /// A recorded chord another shortcut already holds: `(id, key, holder label)`.
    pub conflict: Option<(String, String, String)>,
    /// Every shortcut that fired, oldest first (the live check reads it).
    pub fired: Vec<&'static str>,
}

/// This build's shortcut platform: the desktop app takes the Ctrl defaults, the web client
/// the Alt ones that leave the browser's own Ctrl bindings alone.
pub fn platform() -> Platform {
    Platform {
        desktop: cfg!(not(target_arch = "wasm32")),
        mac: rune::KeyPlatform::current() == rune::KeyPlatform::Mac,
    }
}

/// A Rune key event as the browser's `KeyboardEvent` would report it, or None for a key no
/// shortcut can name.
pub fn chord(key: &KeyEvent, platform: Platform) -> Option<Chord> {
    let named = |s: &str| Some(s.to_owned());
    let (name, letter) = match &key.key {
        Key::Text(t) => (
            Some(t.clone()),
            t.chars()
                .next()
                .filter(|c| t.len() == 1 && c.is_ascii_alphabetic()),
        ),
        Key::Command(c) => (
            Some(c.to_string()),
            Some(*c).filter(char::is_ascii_alphabetic),
        ),
        Key::Enter => (named("Enter"), None),
        Key::Backspace => (named("Backspace"), None),
        Key::Delete => (named("Delete"), None),
        Key::Left => (named("ArrowLeft"), None),
        Key::Right => (named("ArrowRight"), None),
        Key::Up => (named("ArrowUp"), None),
        Key::Down => (named("ArrowDown"), None),
        Key::Home => (named("Home"), None),
        Key::End => (named("End"), None),
        Key::PageUp => (named("PageUp"), None),
        Key::PageDown => (named("PageDown"), None),
        Key::Escape => (named("Escape"), None),
        Key::Tab => (named("Tab"), None),
        Key::Function(n) => (Some(format!("F{n}")), None),
        _ => (None, None),
    };
    let m = key.mods;
    let (ctrl, meta) = if platform.mac {
        (m.ctrl, m.cmd)
    } else {
        (m.ctrl || m.cmd, m.logo && !m.ctrl)
    };
    Some(Chord {
        key: name?,
        code: letter
            .map(|c| format!("Key{}", c.to_ascii_uppercase()))
            .unwrap_or_default(),
        ctrl_key: ctrl,
        meta_key: meta,
        shift_key: m.shift,
        alt_key: m.alt,
    })
}

/// `AppSpec::on_key`: a recording row takes the key first, then Escape closes the top overlay,
/// then a shortcut fires. The theme picker's search field takes its own arrows and Return.
pub fn on_key(s: &mut State, key: &KeyEvent, tx: &mut Tx) -> bool {
    if s.keys.recording.is_some() {
        record(s, key);
        return true;
    }
    if matches!(key.key, Key::Escape) && s.close_top() {
        return true;
    }
    if matches!(key.key, Key::Escape) && s.close_focused_find(tx) {
        return true;
    }
    if matches!(key.key, Key::Escape) && s.close_terminal_assist() {
        return true;
    }
    if s.question_key(key) {
        return true;
    }
    if matches!(key.key, Key::Escape) {
        return false;
    }
    let Some(chord) = chord(key, s.shortcuts.platform) else {
        return false;
    };
    let Some(id) = shortcuts::find_match(&s.shortcuts, &chord).map(|sc| sc.id.clone()) else {
        return false;
    };
    s.fire(&id, tx);
    true
}

fn record(s: &mut State, key: &KeyEvent) {
    let Some(id) = s.keys.recording.clone() else {
        return;
    };
    if matches!(key.key, Key::Escape) {
        s.keys.recording = None;
        s.keys.conflict = None;
        return;
    }
    let Some(chord) = chord(key, s.shortcuts.platform) else {
        return;
    };
    let normalized = shortcuts::normalize_key_event(&chord, s.shortcuts.platform);
    let only_modifiers = normalized
        .split('+')
        .all(|p| ["ctrl", "cmd", "meta", "alt", "shift"].contains(&p));
    if normalized.is_empty() || only_modifiers {
        return;
    }
    match find_conflicting_shortcut(&s.shortcuts.shortcuts, &normalized, &id) {
        Some(holder) => s.keys.conflict = Some((id, normalized, holder.label.clone())),
        None => s.rebind(&id, normalized),
    }
}

impl State {
    /// Binds `id` to `key`, saves the bindings and stops recording.
    pub fn rebind(&mut self, id: &str, key: String) {
        shortcuts::step(
            &mut self.shortcuts,
            Event::UpdateShortcut(id.to_owned(), key),
        );
        self.keys.recording = None;
        self.keys.conflict = None;
        self.save_shortcuts();
    }

    fn reset_shortcut(&mut self, id: &str) {
        shortcuts::step(&mut self.shortcuts, Event::ResetShortcut(id.to_owned()));
        self.save_shortcuts();
    }

    pub(crate) fn save_shortcuts(&mut self) {
        let data = serde_json::to_value(&self.shortcuts.shortcuts).unwrap_or(Value::Null);
        self.write_shared(Shared::Shortcuts, data);
    }

    pub(crate) fn shortcuts_loaded(&mut self, saved: Option<Value>) {
        let saved = saved.unwrap_or_else(|| json!({}));
        let merged = Shortcuts::merged(&saved, self.shortcuts.platform);
        shortcuts::step(&mut self.shortcuts, Event::SetShortcuts(merged));
    }

    /// Runs one shortcut's action. Actions whose screens port in later units say so.
    pub fn fire(&mut self, id: &str, tx: &mut Tx) {
        use model::shell_layout::Event as Layout;
        let Some(known) = shortcuts::defaults(self.shortcuts.platform)
            .0
            .into_iter()
            .find(|(slot, _)| slot == id)
        else {
            return;
        };
        let label = known.1.label;
        log::info!("[keys] shortcut {id}");
        if let Some(fixed) = QUICK.iter().chain(OTHER.iter()).find(|q| **q == id) {
            self.keys.fired.push(fixed);
        }
        match id {
            "sidebarToggle" => {
                let expanded = self.layout.record.rail_expanded;
                self.layout_step(Layout::SetRailExpanded(json!(!expanded)));
            }
            "toggleFileExplorer" => {
                let visible = self.layout.record.side_panel_visible;
                self.layout_step(Layout::SetSidePanelVisible(json!(!visible)));
            }
            "commandPalette" | "commandPaletteAlt" => self.open_palette(false),
            "colorThemePicker" => self.open_theme_picker(),
            "settings" => self.open_settings(None),
            "commandHistory" => self.open_command_history(),
            "terminalSearch" => {
                if !self.open_terminal_find(tx) {
                    self.toast_later(&label);
                }
            }
            "newProject" => self.overlays.open(crate::rail::NEW_PROJECT),
            "newBrowserTab" => self.open_browser_tab(None, None),
            "zoomIn" => self.set_zoom(self.zoom.zoom_in()),
            "zoomOut" => self.set_zoom(self.zoom.zoom_out()),
            "zoomReset" => self.set_zoom(ui::Zoom::DEFAULT),
            "saveFile" => {
                if !self.save_active_canvas() {
                    self.toast_later(&label);
                }
            }
            "worktreeSwitchRoot" | "worktreeSwitchNext" | "worktreeSwitchPrev" => {
                self.toast_later(&label);
            }
            _ => {
                let _ = tx;
                self.toast_later(&label);
            }
        }
    }

    fn set_zoom(&mut self, zoom: ui::Zoom) {
        self.zoom = zoom;
        // Two decimals as the web client stores them, not the f32's widened digits.
        let level = (f64::from(zoom.factor()) * 100.0).round() / 100.0;
        self.set_setting("uiZoomLevel", json!(level));
    }
}

/// The shortcuts outside the menu, for the fired log's `&'static` names.
const OTHER: [&str; 15] = [
    "commandPaletteAlt",
    "terminalSearch",
    "newProject",
    "nextTerminal",
    "prevTerminal",
    "closeTab",
    "saveFile",
    "fileExplorerRename",
    "fileExplorerDelete",
    "worktreeCreate",
    "worktreeSwitchNext",
    "worktreeSwitchPrev",
    "worktreeOpenTerminal",
    "worktreeSwitchRoot",
    "colorThemePicker",
];

/// The shortcut menu: a card 512 px wide at most, 7 % of the window from the top, over a 40 %
/// wash. Each row shows the label, the description and the binding; a press on the binding
/// records the next chord.
pub fn build(s: &State, t: &'static Tokens, height: f32, layer: &mut Vec<El>) {
    if !s.overlays.is_open(MENU) {
        return;
    }
    let c = &t.colors;
    let platform = s.shortcuts.platform;
    let keyboard = theme::icons::named("Keyboard").expect("bundled icon");
    let reset = theme::icons::named("RotateCcw").expect("bundled icon");
    let header = row()
        .key("shortcut-header")
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(12.0)
        .items_start()
        .justify_between()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            col()
                .child(
                    row()
                        .gap(8.0)
                        .items_center()
                        .child(icon(keyboard, 15.0).color(c.foreground).a11y_hidden())
                        .child(
                            text("Shortcut Menu")
                                .font(sans(14.0, 600.0, 20.0))
                                .color(c.foreground)
                                .role("heading"),
                        ),
                )
                .child(
                    text("View and edit common workspace shortcuts.")
                        .font(sans(12.0, 400.0, 16.0))
                        .color(c.muted_foreground)
                        .mt(4.0),
                ),
        )
        .child(
            row()
                .key("shortcut-esc")
                .role("button")
                .label("Esc")
                .px(8.0)
                .py(4.0)
                .radius(4.0)
                .child(
                    text("Esc")
                        .font(sans(12.0, 500.0, 16.0))
                        .color(c.muted_foreground),
                )
                .on_press(|s: &mut State| s.close(MENU)),
        );
    let mut list = col().key("shortcut-list").w_full().px(8.0).py(8.0).gap(4.0);
    for id in QUICK
        .iter()
        .filter(|id| platform.desktop || **id != "newBrowserTab")
    {
        let Some(sc) = s.shortcuts.shortcuts.get(id) else {
            continue;
        };
        let recording = s.keys.recording.as_deref() == Some(*id);
        let conflict = s.keys.conflict.as_ref().filter(|(cid, _, _)| cid == id);
        let customized = sc.custom_key.is_some();
        let shown = match (recording, conflict) {
            (_, Some((_, key, _))) => format_key_for_display(key, platform),
            (true, None) => String::new(),
            _ => format_key_for_display(sc.active_key(), platform),
        };
        let mut cell = row()
            .key(format!("record-{id}"))
            .role("button")
            .label(format!("Record {} shortcut", sc.label))
            .min_w(88.0)
            .min_h(26.0)
            .px(8.0)
            .py(4.0)
            .justify_center()
            .items_center()
            .radius(6.0)
            .border(
                1.0,
                if conflict.is_some() {
                    c.destructive
                } else if recording {
                    c.primary
                } else {
                    c.border
                },
            )
            .fill(if recording {
                c.primary.alpha(0.1)
            } else {
                c.secondary.alpha(0.5)
            });
        cell = if recording && conflict.is_none() {
            cell.child(
                text("Press keys…")
                    .font(sans(12.0, 400.0, 16.0))
                    .color(c.muted_foreground),
            )
        } else {
            cell.child(
                text(shown)
                    .font(mono(11.0, 400.0, 16.0))
                    .color(if customized { c.primary } else { c.foreground }),
            )
        };
        let rec_id = (*id).to_owned();
        let cell = cell.on_press(move |s: &mut State| {
            s.keys.conflict = None;
            s.keys.recording = Some(rec_id.clone());
        });
        let mut line = row().w_full().gap(8.0).items_center().child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text(sc.label.clone())
                        .font(sans(12.0, 500.0, 16.0))
                        .color(c.secondary_foreground)
                        .truncate(),
                )
                .child(
                    text(sc.description.clone())
                        .font(sans(12.0, 400.0, 16.0))
                        .color(c.muted_foreground)
                        .truncate(),
                ),
        );
        if customized {
            let reset_id = (*id).to_owned();
            line = line.child(
                row()
                    .key(format!("reset-{id}"))
                    .role("button")
                    .label(format!("Reset {} shortcut to default", sc.label))
                    .padding(4.0)
                    .radius(4.0)
                    .child(icon(reset, 13.0).color(c.muted_foreground))
                    .on_press(move |s: &mut State| s.reset_shortcut(&reset_id)),
            );
        }
        let mut entry = col()
            .key(format!("shortcut-{id}"))
            .w_full()
            .px(8.0)
            .py(6.0)
            .radius(6.0)
            .child(line.child(cell));
        if let Some((cid, key, holder)) = conflict {
            let (cid, key) = (cid.clone(), key.clone());
            entry = entry.child(
                row()
                    .mt(4.0)
                    .gap(4.0)
                    .child(
                        text(format!("Conflicts with \"{holder}\"."))
                            .font(sans(11.0, 400.0, 16.0))
                            .color(c.destructive),
                    )
                    .child(
                        rune::rich(
                            "Use anyway",
                            vec![rune::Span::new(0, 10).underline(c.destructive)],
                        )
                        .font(sans(11.0, 400.0, 16.0))
                        .color(c.destructive)
                        .role("button")
                        .label("Use anyway")
                        .on_press(move |s: &mut State| s.rebind(&cid, key.clone())),
                    ),
            );
        }
        list = list.child(entry);
    }
    let card = col()
        .key("shortcut-menu-panel")
        .role("dialog")
        .label("Shortcut Menu")
        .w_full()
        .max_w(512.0)
        .radius(8.0)
        .clip_rounded(8.0)
        .fill(c.card)
        .border(1.0, c.border)
        .child(header)
        .child(
            col()
                .w_full()
                .max_h(height * 0.52)
                .child(list)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                }),
        )
        .modal(true, |s: &mut State| s.close(MENU));
    layer.push(
        rune::el()
            .key("shortcut-menu-scrim")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(c.overlay.alpha(0.4))
            .backdrop_blur(4.0)
            .a11y_hidden(),
    );
    layer.push(
        col()
            .key("shortcut-menu-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .items_center()
            .px(16.0)
            .pt((height * 0.07).max(16.0))
            .child(ui::paint::shadows(card, &t.shadows.elevation_2xl)),
    );
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
