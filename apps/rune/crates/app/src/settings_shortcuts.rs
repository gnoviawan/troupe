//! The Keyboard shortcuts and About pages of App Preferences.

use model::shortcuts::{Shortcut, format_key_for_display};
use rune::{El, col, row, text};
use theme::Tokens;
use ui::settings::{card, group, setting};
use ui::type_::sans;
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::settings_view::{RESET_ALL, RESET_SHORTCUTS};

/// `SHORTCUT_GROUPS`, in order; ids not listed fall into Other.
const SHORTCUT_GROUPS: [(&str, &[&str]); 5] = [
    (
        "General",
        &[
            "commandPalette",
            "commandPaletteAlt",
            "commandHistory",
            "newTerminal",
            "colorThemePicker",
            "settings",
        ],
    ),
    (
        "Tabs and panels",
        &[
            "newBrowserTab",
            "nextTerminal",
            "prevTerminal",
            "closeTab",
            "sidebarToggle",
            "toggleFileExplorer",
        ],
    ),
    ("Zoom", &["zoomIn", "zoomOut", "zoomReset"]),
    (
        "Files and search",
        &[
            "newProject",
            "saveFile",
            "fileExplorerRename",
            "fileExplorerDelete",
            "terminalSearch",
        ],
    ),
    (
        "Worktrees",
        &[
            "worktreeCreate",
            "worktreeSwitchNext",
            "worktreeSwitchPrev",
            "worktreeOpenTerminal",
            "worktreeSwitchRoot",
        ],
    ),
];

fn shortcut_row(s: &State, t: &Tokens, shortcut: &Shortcut) -> El {
    let c = &t.colors;
    let id = shortcut.id.clone();
    let recording = s.keys.recording.as_deref() == Some(id.as_str());
    let key = shortcut.custom_key.clone().unwrap_or_else(|| shortcut.default_key.clone());
    let mut keys = row().key("keys").gap(4.0).items_center();
    if recording {
        keys = keys.child(text("Press keys...").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    } else if key.is_empty() {
        keys = keys.child(text("Not set").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    } else {
        for part in format_key_for_display(&key, s.shortcuts.platform).split('+') {
            keys = keys.child(ui::kbd(t, part));
        }
    }
    let rec = id.clone();
    let mut r = row()
        .key(format!("shortcut-{id}"))
        .role("button")
        .label(shortcut.label.clone())
        .w_full()
        .px(16.0)
        .h(44.0)
        .gap(12.0)
        .items_center()
        .focusable(true)
        .hover(c.foreground.alpha(0.03))
        .child(
            text(shortcut.label.clone())
                .font(sans(12.0, 500.0, 16.0))
                .color(c.foreground)
                .grow(1.0)
                .min_w(0.0),
        )
        .child(keys)
        .on_press(move |s: &mut State| {
            s.keys.recording = Some(rec.clone());
            s.keys.conflict = None;
        });
    if recording {
        r = r.fill(c.foreground.alpha(0.04));
    }
    r
}

pub fn shortcuts(s: &State, t: &'static Tokens) -> Vec<El> {
    let query = s.settings_ui.shortcut_filter.text.trim().to_lowercase();
    let visible: Vec<&Shortcut> = s
        .shortcuts
        .shortcuts
        .0
        .iter()
        .map(|(_, sc)| sc)
        .filter(|sc| s.shortcuts.platform.desktop || sc.id != "newBrowserTab")
        .filter(|sc| query.is_empty() || sc.label.to_lowercase().contains(&query) || sc.description.to_lowercase().contains(&query))
        .collect();
    let filter = ui::panel_field(t, &s.settings_ui.shortcut_filter, "Filter shortcuts", false)
        .key("shortcut-filter")
        .h(36.0)
        .grow(1.0)
        .on_edit(|s: &mut State| &mut s.settings_ui.shortcut_filter);
    let reset = Button::new("Reset all")
        .variant(ButtonVariant::Secondary)
        .icon(theme::icons::named("RotateCcw").expect("bundled icon"))
        .build(t)
        .key("reset-shortcuts")
        .label("Reset all shortcuts")
        .on_press(|s: &mut State| s.overlays.open(RESET_SHORTCUTS));
    let mut out = vec![
        row()
            .key("shortcut-tools")
            .w_full()
            .gap(8.0)
            .items_center()
            .child(filter)
            .child(reset),
    ];
    let known: Vec<&str> = SHORTCUT_GROUPS.iter().flat_map(|(_, ids)| ids.iter().copied()).collect();
    let mut groups: Vec<(&str, Vec<&Shortcut>)> = SHORTCUT_GROUPS
        .iter()
        .map(|(title, ids)| {
            (
                *title,
                ids.iter()
                    .filter_map(|id| visible.iter().find(|sc| sc.id == *id).copied())
                    .collect(),
            )
        })
        .collect();
    groups.push((
        "Other",
        visible.iter().filter(|sc| !known.contains(&sc.id.as_str())).copied().collect(),
    ));
    for (title, items) in groups.into_iter().filter(|(_, items)| !items.is_empty()) {
        let rows = items.into_iter().map(|sc| shortcut_row(s, t, sc)).collect();
        out.push(group(
            t,
            &format!("shortcuts-{title}"),
            title,
            vec![],
            vec![card(t, &format!("card-{title}"), rows)],
        ));
    }
    out
}

pub fn about(t: &'static Tokens) -> Vec<El> {
    let c = &t.colors;
    let version = setting(
        t,
        "server-version",
        &format!("termul-server {}", env!("CARGO_PKG_VERSION")),
        "The web client is served by the termul-server and updates together with it. Reload the page after the server updates.",
        None,
    );
    let logs = group(
        t,
        "logs",
        "Logs and bug reports",
        vec![],
        vec![card(
            t,
            "logs-card",
            vec![setting(
                t,
                "report-bug",
                "Report a bug",
                "File an issue with a redacted diagnostics bundle.",
                Some(
                    Button::new("Report a bug")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Xs)
                        .build(t)
                        .key("report-bug-button")
                        .on_press(|s: &mut State| s.toast_later("Report a bug")),
                ),
            )],
        )],
    );
    let reset = row()
        .key("reset-all")
        .w_full()
        .px(16.0)
        .py(14.0)
        .gap(24.0)
        .items_center()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text("Reset all settings").font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                .child(
                    text("Every setting goes back to its default, including keyboard shortcuts. Terminal profiles stay.")
                        .font(sans(12.0, 400.0, 16.0))
                        .color(c.muted_foreground)
                        .wrap(),
                ),
        )
        .child(
            Button::new("Reset settings")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Xs)
                .ink(c.destructive)
                .build(t)
                .key("reset-settings")
                .on_press(|s: &mut State| s.overlays.open(RESET_ALL)),
        );
    let reset = card(t, "reset-card", vec![reset]);
    vec![card(t, "version-card", vec![version]), logs, reset]
}
