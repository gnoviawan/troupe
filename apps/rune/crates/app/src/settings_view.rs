//! The Settings window (`AppPreferencesModal` in `SettingsWindow` and `SettingsLayout`): a
//! full-window layer with the rail (Back to app, search, grouped pages) and one page at a time
//! in a 640 px column, the page's selects as menus, and the two reset confirms. Phones get the
//! narrow layout: a title bar with Close and a scrolling strip of pages over the page. The
//! page bodies are `settings_pages`.

use rune::{El, TextEdit, col, el, icon, row, text};
use theme::Tokens;
use ui::overlay::{ConfirmDialog, ConfirmVariant};
use ui::settings::pages::{self, APP_PAGES, Owner, Page};
use ui::type_::sans;

use crate::State;

/// The overlay key of the App Preferences window.
pub const APP: &str = "settings-app";
pub const RESET_ALL: &str = "settings-reset-all";
pub const RESET_SHORTCUTS: &str = "settings-reset-shortcuts";
/// The overlay key of an open settings select; [`SettingsUi::select`] says which.
pub const SELECT: &str = "settings-select";

/// The rail's width on a wide window.
const RAIL: f32 = 240.0;
/// The page column's widest.
const COLUMN: f32 = 640.0;

#[derive(Default)]
pub struct SettingsUi {
    /// The App Preferences page; None is the first.
    pub page: Option<&'static str>,
    /// The select whose menu is open.
    pub select: Option<&'static str>,
    pub search: TextEdit,
    pub shortcut_filter: TextEdit,
    /// Disclosures the user opened ("terminal-advanced", "agents-limits").
    pub open: Vec<&'static str>,
}

impl State {
    /// `openApp(page)`: the window on `page` (old ids follow their alias), else the first page.
    pub fn open_settings(&mut self, page: Option<&str>) {
        let id = page.map(pages::resolve_app_page);
        self.settings_ui.page = id.and_then(|id| APP_PAGES.iter().find(|p| p.id == id)).map(|p| p.id);
        self.settings_ui.search = TextEdit::new("");
        self.settings_ui.shortcut_filter = TextEdit::new("");
        log::info!("[settings] open {}", self.settings_ui.page.unwrap_or("preferences"));
        self.overlays.close(crate::DRAWER);
        if !self.overlays.is_open(APP) {
            self.overlays.open(APP);
        }
    }

    pub fn close_settings(&mut self) {
        self.keys.recording = None;
        self.close(APP);
    }

    pub(crate) fn settings_page(&self) -> &'static Page {
        pages::find(&APP_PAGES, self.settings_ui.page.unwrap_or("preferences"))
    }

    pub(crate) fn open_select(&mut self, id: &'static str) {
        self.settings_ui.select = Some(id);
        self.overlays.open(SELECT);
    }

    pub(crate) fn close_select(&mut self) {
        self.settings_ui.select = None;
        self.close(SELECT);
    }

    pub(crate) fn toggle_disclosure(&mut self, id: &'static str) {
        let open = &mut self.settings_ui.open;
        match open.iter().position(|o| *o == id) {
            Some(at) => {
                open.remove(at);
            }
            None => open.push(id),
        }
    }

    /// `handleResetConfirm`: the settings, then the shortcuts.
    fn reset_all_settings(&mut self) {
        log::info!("[settings] reset all settings");
        self.settings_step(model::settings::Event::Reset);
        self.reset_all_shortcuts();
        self.close(RESET_ALL);
    }

    pub(crate) fn reset_all_shortcuts(&mut self) {
        model::shortcuts::step(&mut self.shortcuts, model::shortcuts::Event::ResetAllShortcuts);
        self.save_shortcuts();
        self.close(RESET_SHORTCUTS);
    }
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

/// A 28 px rail row (`RAIL_ROW_CLASS`): a page, or Back to app.
fn rail_row(t: &Tokens, key: String, label: &str, glyph_name: &str, active: bool) -> El {
    rail_item(t, key, label, glyph_name, active).w_full()
}

fn rail_item(t: &Tokens, key: String, label: &str, glyph_name: &str, active: bool) -> El {
    let c = &t.colors;
    let ink = if active { c.foreground } else { c.secondary_foreground };
    let mut r = row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .radius(6.0)
        .items_center()
        .focusable(true)
        .child(icon(glyph(glyph_name), 14.0).color(ink).a11y_hidden())
        .child(
            text(label)
                .font(sans(12.0, 500.0, 16.0))
                .color(ink)
                .single_line()
                .truncate()
                .min_w(0.0),
        );
    if active {
        r = r.fill(c.foreground.alpha(0.1));
    } else {
        r = r.hover(c.foreground.alpha(0.04));
    }
    r
}

fn rail(s: &State, t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let active = s.settings_page().id;
    let mut nav = col()
        .key("settings-nav")
        .role("navigation")
        .label("Settings pages")
        .w_full()
        .gap(1.0)
        .px(12.0)
        .pb(12.0);
    let mut last_group = None;
    for page in &APP_PAGES {
        if page.group != last_group && !phone {
            if let Some(group) = page.group {
                nav = nav.child(
                    row()
                        .key(format!("group-{group}"))
                        .w_full()
                        .px(8.0)
                        .pt(16.0)
                        .pb(4.0)
                        .child(text(group).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)),
                );
            }
            last_group = page.group;
        }
        let id = page.id;
        nav = nav.child(
            rail_row(t, format!("page-{id}"), page.label, page.icon, id == active).on_press(move |s: &mut State| {
                log::info!("[settings] page {id}");
                s.settings_ui.page = Some(id);
                s.keys.recording = None;
            }),
        );
    }
    let search = ui::panel_field(t, &s.settings_ui.search, "Search settings...", false)
        .key("settings-search")
        .w_full()
        .h(28.0)
        .pl(30.0)
        .on_edit(|s: &mut State| &mut s.settings_ui.search);
    let search = row().key("settings-search-band").w_full().px(12.0).pt(4.0).pb(8.0).child(
        row().key("field").w_full().child(search).child(
            icon(glyph("Search"), 13.0)
                .color(c.muted_foreground)
                .pin(Some(11.0), None, Some(8.5), None)
                .a11y_hidden(),
        ),
    );
    let mut aside = col()
        .key("settings-rail")
        .w(if phone { 0.0 } else { RAIL })
        .h_full()
        .fill(c.sidebar_background)
        .items_stretch();
    if phone {
        return aside;
    }
    aside =
        aside
            .child(el().key("top").h(44.0))
            .child(
                col().key("back-band").px(12.0).pb(4.0).child(
                    rail_row(t, "settings-back".into(), "Back to app", "ArrowLeft", false).on_press(|s: &mut State| s.close_settings()),
                ),
            )
            .child(search)
            .child(ui::scroll_area(t, "settings-nav-scroll", nav));
    aside
}

/// The narrow layout's top: the title and Close, then the pages as a scrolling strip.
fn phone_bar(s: &State, t: &'static Tokens) -> El {
    let c = &t.colors;
    let active = s.settings_page().id;
    let mut strip = row()
        .key("settings-strip")
        .role("navigation")
        .label("Settings pages")
        .gap(1.0)
        .px(12.0)
        .pb(8.0);
    for page in &APP_PAGES {
        let id = page.id;
        strip = strip.child(
            rail_item(t, format!("page-{id}"), page.label, page.icon, id == active)
                .h(44.0)
                .on_press(move |s: &mut State| s.settings_ui.page = Some(id)),
        );
    }
    col()
        .key("settings-phone-bar")
        .w_full()
        .fill(c.sidebar_background)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            row()
                .key("title")
                .w_full()
                .min_h(48.0)
                .pl(16.0)
                .pr(10.0)
                .items_center()
                .child(text("Preferences").font(sans(14.0, 600.0, 20.0)).color(c.foreground).grow(1.0))
                .child(
                    ui::quiet_icon_button(t, glyph("X"), "Close Preferences", 44.0, 16.0)
                        .key("settings-close")
                        .on_press(|s: &mut State| s.close_settings()),
                ),
        )
        .child(row().key("strip-scroll").w_full().scroll_x().child(strip))
}

fn page_body(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> El {
    let page = s.settings_page();
    let mut column = col()
        .key(format!("settings-page-{}", page.id))
        .role("region")
        .label(page.label)
        .w_full()
        .max_w(COLUMN)
        .items_stretch()
        .child(ui::settings::header(t, page.label, page.description));
    let groups = match page.owner {
        Owner::Here => crate::settings_pages::page(s, t, page.id, layer),
        Owner::Later(unit) => vec![ui::settings::notice(
            t,
            "later",
            &format!("This page is not in the Rune client yet; {unit} ports it. Open it in the web client meanwhile."),
        )],
    };
    column = column.child(col().key("groups").w_full().gap(32.0).items_stretch().children(groups));
    ui::scroll_area(
        t,
        "settings-page-scroll",
        row()
            .key("settings-page-pad")
            .w_full()
            .px(32.0)
            .pt(16.0)
            .pb(80.0)
            .justify_center()
            .child(column),
    )
}

pub fn build(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) {
    if !s.overlays.is_open(APP) {
        return;
    }
    let c = &t.colors;
    let mut frame = if phone { col() } else { row() }
        .key("settings-window")
        .role("dialog")
        .label("Preferences")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(c.background)
        .items_stretch();
    let mut inner = Vec::new();
    let body = page_body(s, t, &mut inner);
    frame = if phone {
        frame
            .child(phone_bar(s, t))
            .child(col().key("settings-main").grow(1.0).min_h(0.0).child(body))
    } else {
        frame
            .child(rail(s, t, false))
            .child(col().key("settings-main").grow(1.0).min_w(0.0).h_full().child(body))
    };
    layer.push(frame);
    layer.extend(inner);
    ConfirmDialog::new(
        RESET_ALL,
        "Reset all settings?",
        "Every setting goes back to its default, including keyboard shortcuts. Terminal profiles stay. You cannot undo this.",
    )
    .variant(ConfirmVariant::Danger)
    .confirm_label("Reset settings")
    .cancel_label("Cancel")
    .open(s.overlays.is_open(RESET_ALL))
    .on_cancel(|s: &mut State| s.close(RESET_ALL))
    .on_confirm(|s: &mut State| s.reset_all_settings())
    .build(t, layer);
    ConfirmDialog::new(
        RESET_SHORTCUTS,
        "Reset all shortcuts?",
        "Every keyboard shortcut goes back to its default keys. You cannot undo this.",
    )
    .variant(ConfirmVariant::Danger)
    .confirm_label("Reset shortcuts")
    .cancel_label("Cancel")
    .open(s.overlays.is_open(RESET_SHORTCUTS))
    .on_cancel(|s: &mut State| s.close(RESET_SHORTCUTS))
    .on_confirm(|s: &mut State| s.reset_all_shortcuts())
    .build(t, layer);
}
