//! The Settings window (`AppPreferencesModal` in `SettingsWindow` and `SettingsLayout`): a
//! full-window layer with the rail (Back to app, search, grouped pages) and one page at a time
//! in a 640 px column, the page's selects as menus, and the two reset confirms. Phones get the
//! narrow layout: a title bar with Close and a scrolling strip of pages over the page. The
//! page bodies are `settings_pages`.

use rune::{El, TextEdit, col, el, icon, row, text};
use theme::Tokens;
use ui::overlay::{ConfirmDialog, ConfirmVariant};
use ui::settings::pages::{self, APP_PAGES, Owner, PROJECT_PAGES, Page};
use ui::type_::sans;

use crate::State;
use crate::project_settings::{LEAVE, PROJECT};

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
    pub profile: crate::settings_profiles::ProfileEditor,
    pub skills: crate::settings_profiles::SkillsUi,
    /// The shells `/shells` detected; empty while the read is out.
    pub shells: Option<Vec<crate::settings_profiles::ShellInfo>>,
    /// The undo toast on screen and what its Undo puts back.
    pub undo: Option<(u64, crate::settings_profiles::Undo)>,
}

/// Which Settings window is open: App Preferences or a project's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Window {
    App,
    Project,
}

impl Window {
    fn pages(self) -> &'static [Page] {
        match self {
            Window::App => &APP_PAGES,
            Window::Project => &PROJECT_PAGES,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Window::App => "Preferences",
            Window::Project => "Project settings",
        }
    }
}

impl State {
    /// The Settings window on screen, if any.
    pub(crate) fn settings_window(&self) -> Option<Window> {
        if self.overlays.is_open(APP) {
            Some(Window::App)
        } else if self.overlays.is_open(PROJECT) {
            Some(Window::Project)
        } else {
            None
        }
    }

    fn window_page(&self, w: Window) -> &'static Page {
        match w {
            Window::App => self.settings_page(),
            Window::Project => pages::find(&PROJECT_PAGES, self.project_ui.page.unwrap_or("general")),
        }
    }

    fn pick_page(&mut self, w: Window, id: &'static str) {
        log::info!("[settings] page {id}");
        match w {
            Window::App => {
                self.settings_ui.page = Some(id);
                self.keys.recording = None;
                self.settings_page_opened();
            }
            Window::Project => self.project_ui.page = Some(id),
        }
    }

    fn close_window(&mut self, w: Window) {
        match w {
            Window::App => self.close_settings(),
            Window::Project => self.close_project_settings(),
        }
    }

    /// `openApp(page)`: the window on `page` (old ids follow their alias), else the first page.
    pub fn open_settings(&mut self, page: Option<&str>) {
        let id = page.map(pages::resolve_app_page);
        self.settings_ui.page = id.and_then(|id| APP_PAGES.iter().find(|p| p.id == id)).map(|p| p.id);
        self.settings_ui.search = TextEdit::new("");
        self.settings_ui.shortcut_filter = TextEdit::new("");
        log::info!("[settings] open {}", self.settings_ui.page.unwrap_or("preferences"));
        self.settings_page_opened();
        self.overlays.close(crate::DRAWER);
        if !self.overlays.is_open(APP) {
            self.overlays.open(APP);
        }
    }

    /// Reads what the shown page lists, once per window.
    pub(crate) fn settings_page_opened(&mut self) {
        if self.settings_page().id == "skills" {
            let root = self.active_project().and_then(|p| p.path.clone()).unwrap_or_default();
            self.fetch_skills_for(root);
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

fn rail(s: &State, t: &'static Tokens, w: Window) -> El {
    let c = &t.colors;
    let active = s.window_page(w).id;
    let mut nav = col()
        .key("settings-nav")
        .role("navigation")
        .label("Settings pages")
        .w_full()
        .gap(1.0)
        .px(12.0)
        .pb(12.0);
    let mut last_group = None;
    for page in w.pages() {
        if page.group != last_group {
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
            rail_row(t, format!("page-{id}"), page.label, page.icon, id == active).on_press(move |s: &mut State| s.pick_page(w, id)),
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
    let mut back = col().key("back-band").px(12.0).pb(4.0).gap(4.0).child(
        rail_row(t, "settings-back".into(), "Back to app", "ArrowLeft", false).on_press(move |s: &mut State| s.close_window(w)),
    );
    if let Some(subtitle) = subtitle(s, w) {
        back = back.child(
            col()
                .key("rail-title")
                .w_full()
                .px(8.0)
                .pt(8.0)
                .pb(4.0)
                .child(text(w.title()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).truncate())
                .child(text(subtitle).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate()),
        );
    }
    col()
        .key("settings-rail")
        .w(RAIL)
        .h_full()
        .fill(c.sidebar_background)
        .items_stretch()
        .child(el().key("top").h(44.0))
        .child(back)
        .child(search)
        .child(ui::scroll_area(t, "settings-nav-scroll", nav))
}

/// The line under the window's title: the project's name.
fn subtitle(s: &State, w: Window) -> Option<String> {
    match w {
        Window::App => None,
        Window::Project => s.settings_project().map(|p| p.name.clone()),
    }
}

/// The narrow layout's top: the title and Close, then the pages as a scrolling strip.
fn phone_bar(s: &State, t: &'static Tokens, w: Window) -> El {
    let c = &t.colors;
    let active = s.window_page(w).id;
    let mut strip = row()
        .key("settings-strip")
        .role("navigation")
        .label("Settings pages")
        .gap(1.0)
        .px(12.0)
        .pb(8.0);
    for page in w.pages() {
        let id = page.id;
        strip = strip.child(
            rail_item(t, format!("page-{id}"), page.label, page.icon, id == active)
                .h(44.0)
                .on_press(move |s: &mut State| s.pick_page(w, id)),
        );
    }
    let mut title = col().key("words").grow(1.0).min_w(0.0).py(8.0).child(text(w.title()).font(sans(14.0, 600.0, 20.0)).color(c.foreground).truncate());
    if let Some(subtitle) = subtitle(s, w) {
        title = title.child(text(subtitle).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate());
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
                .child(title)
                .child(
                    ui::quiet_icon_button(t, glyph("X"), format!("Close {}", w.title()), 44.0, 16.0)
                        .key("settings-close")
                        .on_press(move |s: &mut State| s.close_window(w)),
                ),
        )
        .child(row().key("strip-scroll").w_full().scroll_x().child(strip))
}

fn page_body(s: &State, t: &'static Tokens, w: Window, phone: bool, layer: &mut Vec<El>) -> El {
    let page = s.window_page(w);
    let mut column = col()
        .key(format!("settings-page-{}", page.id))
        .role("region")
        .label(page.label)
        .w_full()
        .max_w(COLUMN)
        .items_stretch()
        .child(ui::settings::header(t, page.label, page.description));
    let groups = match page.owner {
        Owner::Here if w == Window::Project => crate::project_settings_view::page(s, t, page.id, phone, layer),
        Owner::Here => crate::settings_pages::page(s, t, page.id, phone, layer),
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
    let Some(w) = s.settings_window() else {
        return;
    };
    let c = &t.colors;
    let mut frame = if phone { col() } else { row() }
        .key("settings-window")
        .role("dialog")
        .label(w.title())
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(c.background)
        .items_stretch();
    let mut inner = Vec::new();
    let body = page_body(s, t, w, phone, &mut inner);
    let mut main = col().key("settings-main").grow(1.0).items_stretch().child(col().key("settings-body").grow(1.0).min_h(0.0).child(body));
    if w == Window::Project && s.project_dirty() {
        main = main.child(crate::project_settings_view::footer(t, phone));
    }
    frame = if phone {
        frame.child(phone_bar(s, t, w)).child(main.min_h(0.0))
    } else {
        frame.child(rail(s, t, w)).child(main.min_w(0.0).h_full())
    };
    layer.push(frame);
    layer.extend(inner);
    if w == Window::Project {
        let name = s.settings_project().map_or_else(|| "this project".to_owned(), |p| p.name.clone());
        ConfirmDialog::new(LEAVE, "Unsaved changes", format!("You have unsaved changes in {name}. Leaving will discard them."))
            .variant(ConfirmVariant::Danger)
            .confirm_label("Leave")
            .cancel_label("Cancel")
            .open(s.overlays.is_open(LEAVE))
            .on_cancel(|s: &mut State| s.close(LEAVE))
            .on_confirm(|s: &mut State| s.leave_project_settings())
            .build(t, layer);
        return;
    }
    crate::settings_profiles::editor(s, t, layer);
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
