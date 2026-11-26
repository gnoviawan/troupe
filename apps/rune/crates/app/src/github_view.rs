//! The GitHub side panel (`GithubPanel.tsx`): the onboarding states until `gh` is installed,
//! signed in and the remote resolves, then the repository select of a fork, the auth warning,
//! the filters (`GithubFilters.tsx`) and the list. The same body fills the phone's GitHub sheet.
//! The multi-select picker (`GithubPicker.tsx`) lives here too; the item tab opens it as well.

use model::github::{
    AuthStatus, FilterPatch, KindFilter, LabelMode, ListFilter, RepoResolution, RepoSlot, Sort, StateFilter, Status,
    TokenSource,
};
use rune::{El, Tx, col, el, icon, row, text};
use theme::Tokens;
use ui::git::glyph;
use ui::overlay::{Menu, MenuItem, Placement, Popover};
use ui::type_::sans;
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::github::{
    ASSIGNEE_MENU, Blocker, PICKER, PickerFor, REPO_SELECT, SORT_MENU, STATE_MENU, blocker,
};
use crate::github_logic as logic;

const STATES: [(StateFilter, &str); 5] = [
    (StateFilter::Open, "Open"),
    (StateFilter::Closed, "Closed"),
    (StateFilter::Merged, "Merged"),
    (StateFilter::Draft, "Draft"),
    (StateFilter::All, "All states"),
];
const SORTS: [(Sort, &str); 3] = [(Sort::Updated, "Recently updated"), (Sort::Created, "Newest"), (Sort::Comments, "Most commented")];
const KINDS: [(KindFilter, &str); 3] = [(KindFilter::All, "All"), (KindFilter::Issue, "Issues"), (KindFilter::Pr, "PRs")];
const MODES: [(LabelMode, &str); 3] = [(LabelMode::Any, "Match any"), (LabelMode::All, "Match all"), (LabelMode::Exclude, "Without")];
/// GitHub's default label colours, offered when creating a label.
pub const LABEL_SWATCHES: [&str; 8] = ["b60205", "d93f0b", "fbca04", "0e8a16", "006b75", "1d76db", "0052cc", "5319e7"];

fn note(t: &Tokens, line: &str) -> El {
    text(line.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap().w_full()
}

fn skeleton_list(t: &Tokens) -> El {
    let mut list = col().role("status").label("Loading").w_full().gap(8.0).px(12.0).py(8.0);
    for _ in 0..4 {
        list = list.child(ui::base::skeleton::skeleton(t).w_full().h(40.0).radius(8.0));
    }
    list
}

/// `PanelMessage`: a failed read and its Retry.
fn panel_message(t: &Tokens, key: &str, title: &str, detail: Option<&str>, retry: impl Fn(&mut State) + 'static) -> El {
    let mut message = col()
        .key(key.to_owned())
        .role("alert")
        .w_full()
        .items_center()
        .gap(8.0)
        .px(16.0)
        .py(24.0)
        .child(text(title.to_owned()).font(sans(14.0, 500.0, 20.0)).color(t.colors.destructive));
    if let Some(detail) = detail {
        message = message.child(text(detail.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap());
    }
    message.child(Button::new("Retry").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key(format!("{key}-retry")).on_press(retry))
}

/// `GithubStatusView`: replaces the list until GitHub is usable for this project.
fn status_view(t: &Tokens, blocker: &Blocker, retrying: bool, touch: bool) -> El {
    let c = &t.colors;
    let retry = Button::new(if retrying { "Checking\u{2026}" } else { "Retry" })
        .variant(ButtonVariant::Outline)
        .size(if touch { ButtonSize::Touch } else { ButtonSize::Sm })
        .disabled(retrying)
        .build(t)
        .key("github-status-retry")
        .on_press(|s: &mut State| s.github_retry_context());
    let title = |line: String| text(line).font(sans(14.0, 500.0, 20.0)).color(c.foreground).wrap();
    let mut view = col()
        .key("github-status")
        .w_full()
        .items_center()
        .gap(12.0)
        .px(16.0)
        .py(32.0)
        .child(icon(glyph("Github"), 28.0).color(c.muted_foreground));
    view = match blocker {
        Blocker::CliMissing => view
            .child(title("GitHub CLI not installed".into()))
            .child(note(t, "Troupe reads issues and pull requests through the GitHub CLI (gh) on the machine running Troupe. Install it from cli.github.com, then retry."))
            .child(retry),
        Blocker::NotGithub => view
            .child(title("This project isn't a GitHub repository".into()))
            .child(note(t, "Its git remote is missing or points to a host the GitHub CLI doesn't know."))
            .child(retry),
        Blocker::Unauthenticated { host, login_command } => view
            .child(title(format!("Sign in to {host}")))
            .child(note(t, "Troupe uses your GitHub CLI login and never stores a token. Sign in opens a terminal that runs:"))
            .child(
                el().px(8.0).py(4.0).radius(6.0).fill(c.muted).child(
                    text(login_command.clone()).font(ui::type_::mono(11.0, 400.0, 16.0)).color(c.foreground).wrap(),
                ),
            )
            .child(
                row()
                    .gap(8.0)
                    .child(Button::new("Sign in").size(ButtonSize::Sm).build(t).key("github-sign-in").on_press(|s: &mut State| {
                        s.toast_later("Sign in from a terminal");
                    }))
                    .child(retry),
            ),
    };
    view
}

/// `GithubAuthWarning`: missing scopes, or an env token hiding the keyring login.
fn auth_warning(t: &Tokens, auth: &AuthStatus) -> Option<El> {
    let AuthStatus::Connected { host, missing_scopes, token_source, fix_commands, .. } = auth else {
        return None;
    };
    let env = *token_source == TokenSource::Env;
    if missing_scopes.is_empty() && !env && fix_commands.is_empty() {
        return None;
    }
    let mut line = String::new();
    if !missing_scopes.is_empty() {
        let noun = if missing_scopes.len() == 1 { "scope" } else { "scopes" };
        line.push_str(&format!("Your login on {host} is missing the {} {noun}. ", missing_scopes.join(", ")));
    }
    if env {
        line.push_str("GH_TOKEN or GITHUB_TOKEN is set and overrides your gh login. ");
    }
    if !fix_commands.is_empty() {
        line.push_str("Run this to fix it:");
    }
    let c = &t.colors;
    let mut warning = col()
        .key("github-auth-warning")
        .role("status")
        .mx(8.0)
        .mb(8.0)
        .gap(6.0)
        .px(10.0)
        .py(8.0)
        .radius(8.0)
        .fill(c.warning.alpha(0.1))
        .child(
            row()
                .w_full()
                .gap(6.0)
                .child(icon(glyph("AlertTriangle"), 14.0).color(c.warning))
                .child(text(line.trim().to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.warning).wrap().grow(1.0).min_w(0.0)),
        );
    for command in fix_commands {
        warning = warning.child(
            el().w_full().px(8.0).py(4.0).radius(6.0).fill(c.background.alpha(0.6)).child(
                text(command.clone()).font(ui::type_::mono(11.0, 400.0, 16.0)).color(c.foreground).wrap(),
            ),
        );
    }
    Some(warning)
}

/// The panel body for the active project's working copy.
pub fn panel(s: &State, t: &'static Tokens, touch: bool, layer: &mut Vec<El>) -> El {
    let root = col().key("side-panel-github").w_full().grow(1.0).min_h(0.0);
    let Some(cwd) = s.github_cwd() else {
        return root.child(note(t, "Open a project to see its issues and pull requests.").px(16.0).py(24.0));
    };
    let context = s.github.cache.context(&cwd);
    let Some(data) = context.data else {
        if context.status == Status::Error {
            let detail = context.error.clone();
            return root.child(panel_message(t, "github-context-error", "Couldn't check GitHub", detail.as_deref(), |s: &mut State| s.github_retry_context()));
        }
        return root.child(skeleton_list(t));
    };
    let resolved = match (&data.repo, blocker(&data)) {
        (RepoResolution::Resolved { origin, upstream }, None) => (origin.clone(), upstream.clone()),
        (_, found) => {
            let found = found.unwrap_or(Blocker::NotGithub);
            return root.child(status_view(t, &found, context.status == Status::Loading, touch));
        }
    };
    let (origin, upstream) = resolved;
    let filter = s.github.cache.filter(&cwd);
    let list = s.github.cache.list(&cwd);
    let slot = if filter.repo == RepoSlot::Upstream { upstream.as_ref().unwrap_or(&origin) } else { &origin };
    // The list cache is keyed by slot: after a remote change it holds the old repository's items.
    let list_data = list.data.as_ref().filter(|l| l.repo.host == slot.host && l.repo.full_name() == slot.full_name());
    let mut body = root;
    if let Some(upstream) = &upstream {
        body = body.child(repo_select(s, t, &cwd, &filter, &origin.full_name(), &upstream.full_name(), touch, layer));
    }
    if let Some(warning) = auth_warning(t, &data.auth) {
        body = body.child(warning);
    }
    let refreshing = list.status == Status::Loading;
    body = body.child(filters(s, t, &cwd, &filter, refreshing, touch, layer));
    let c = &t.colors;
    if let (Status::Error, Some(data)) = (list.status, list_data) {
        let why = if list.error_code.as_deref() == Some("GH_RATE_LIMITED") { "GitHub rate limit reached." } else { "Couldn't refresh." };
        let line = format!("{why} Showing the list from {}.", logic::relative_ago(&data.fetched_at, crate::git_history::now_ms()));
        body = body.child(
            row()
                .key("github-list-stale")
                .role("status")
                .mx(8.0)
                .mb(4.0)
                .gap(6.0)
                .px(10.0)
                .py(6.0)
                .radius(8.0)
                .fill(c.warning.alpha(0.1))
                .child(icon(glyph("AlertTriangle"), 12.0).color(c.warning))
                .child(text(line).font(sans(11.0, 400.0, 14.0)).color(c.warning).wrap().grow(1.0).min_w(0.0)),
        );
    }
    let Some(data) = list_data else {
        if list.status == Status::Error {
            let title = if list.error_code.as_deref() == Some("GH_RATE_LIMITED") { "GitHub rate limit reached" } else { "Couldn't load issues and pull requests" };
            return body.child(panel_message(t, "github-list-error", title, list.error.as_deref(), |s: &mut State| s.github_refresh_list()));
        }
        return body.child(skeleton_list(t));
    };
    if data.items.is_empty() {
        return body.child(
            col().key("github-empty").w_full().grow(1.0).center().px(16.0).py(24.0).child(note(t, "Nothing matches these filters.")),
        );
    }
    let active = s.github_visible_item();
    let now = crate::git_history::now_ms();
    let mut rows = col().key("github-list").role("list").label("Issues and pull requests").w_full().px(6.0).pb(8.0);
    for item in &data.items {
        let here = active.as_ref().is_some_and(|a| a.number == item.number && a.repo == filter.repo && a.cwd == cwd);
        let updated = crate::git_history::relative_time(&item.updated_at, now);
        let open = item.clone();
        let at = cwd.clone();
        rows = rows.child(
            ui::github::list_row(t, item, here, &updated, touch)
                .key(format!("github-item-{}", item.number))
                .on_press(move |s: &mut State| s.github_open_item(&at, &open)),
        );
    }
    body.child(ui::base::scroll_area::scroll_area(t, "github-list-scroll", rows).grow(1.0).min_h(0.0))
}

/// The fork's repository select: origin, or its upstream.
#[allow(clippy::too_many_arguments)]
fn repo_select(s: &State, t: &'static Tokens, cwd: &str, filter: &ListFilter, origin: &str, upstream: &str, touch: bool, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let current = if filter.repo == RepoSlot::Upstream { format!("{upstream} (upstream)") } else { origin.to_owned() };
    let trigger = row()
        .key("github-repo-trigger")
        .role("combobox")
        .label("Repository")
        .h(if touch { 44.0 } else { 28.0 })
        .grow(1.0)
        .min_w(0.0)
        .px(6.0)
        .gap(4.0)
        .items_center()
        .radius(6.0)
        .hover(c.foreground.alpha(0.03))
        .child(text(current).font(sans(if touch { 14.0 } else { 12.0 }, 500.0, 16.0)).color(c.foreground).single_line().truncate().min_w(0.0))
        .child(icon(glyph("ChevronDown"), 12.0).color(c.muted_foreground))
        .on_press(|s: &mut State| s.overlays.open(REPO_SELECT));
    let pick = |slot: RepoSlot| {
        let cwd = cwd.to_owned();
        move |s: &mut State| {
            s.close(REPO_SELECT);
            s.github_set_filter(&cwd, FilterPatch { repo: Some(slot), ..FilterPatch::default() });
        }
    };
    let menu = Menu::new(REPO_SELECT)
        .width(240.0)
        .placement(Placement::BottomStart)
        .open(s.overlays.is_open(REPO_SELECT))
        .on_close(|s: &mut State| s.close(REPO_SELECT))
        .item(MenuItem::new(origin.to_owned()).checked(filter.repo == RepoSlot::Origin).on_select(pick(RepoSlot::Origin)))
        .item(MenuItem::new(format!("{upstream} (upstream)")).checked(filter.repo == RepoSlot::Upstream).on_select(pick(RepoSlot::Upstream)));
    row().key("github-repo").w_full().h(if touch { 44.0 } else { 32.0 }).px(10.0).items_center().child(menu.build(t, layer, trigger))
}

/// `GithubChoiceMenu`: a single-select chip and its radio menu.
#[allow(clippy::too_many_arguments)]
fn choice_menu(
    s: &State,
    t: &'static Tokens,
    key: &'static str,
    title: &str,
    glyph_name: &str,
    options: Vec<(String, String)>,
    value: &str,
    neutral: &str,
    touch: bool,
    on_pick: impl Fn(&mut State, &str) + Clone + 'static,
    layer: &mut Vec<El>,
) -> El {
    let current = options.iter().find(|(v, _)| v == value).map_or(value.to_owned(), |(_, l)| l.clone());
    let chip = ui::github::filter_chip(t, &format!("{title}: {current}"), &current, Some(glyph_name), value != neutral, 0, touch)
        .key(format!("{key}-chip"))
        .on_press(move |s: &mut State| {
            if key == ASSIGNEE_MENU {
                s.github_meta_for_filter();
            }
            s.overlays.open(key);
        });
    let mut menu = Menu::new(key).name(title.to_owned()).width(176.0).placement(Placement::BottomStart).open(s.overlays.is_open(key)).on_close(move |s: &mut State| s.close(key));
    for (v, label) in options {
        let pick = on_pick.clone();
        let here = v == value;
        menu = menu.item(MenuItem::new(label).checked(here).on_select(move |s: &mut State| {
            s.close(key);
            pick(s, &v);
        }));
    }
    menu.build(t, layer, chip)
}

#[allow(clippy::too_many_arguments)]
fn filters(s: &State, t: &'static Tokens, cwd: &str, filter: &ListFilter, refreshing: bool, touch: bool, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let search = ui::base::panel_styles::panel_field(t, &s.github.search, "Search issues and PRs", touch)
        .key("github-search")
        .label("Search issues and pull requests".to_owned())
        .w_full()
        .h(if touch { 44.0 } else { 32.0 })
        .py(if touch { 10.0 } else { 8.0 })
        .pl(32.0)
        .on_edit(|s: &mut State| {
            s.github_search_edited();
            &mut s.github.search
        })
        .on_submit(|s: &mut State, _tx: &mut Tx| s.github_search_commit());
    let search = el()
        .w_full()
        .child(search)
        .child(el().absolute(11.0, if touch { 15.0 } else { 9.5 }).a11y_hidden().child(icon(glyph("Search"), 13.0).color(c.muted_foreground)));
    let kinds: Vec<&str> = KINDS.iter().map(|(_, l)| *l).collect();
    let active = KINDS.iter().position(|(k, _)| *k == filter.kind).unwrap_or(0);
    let kind_cwd = cwd.to_owned();
    let segment = ui::base::panel_styles::Segmented::new("Kind", &kinds, active)
        .build(t, "github-kind", false, move |s: &mut State, i: usize| {
            let filter = s.github.cache.filter(&kind_cwd);
            s.github_set_filter(&kind_cwd, logic::kind_patch(&filter, KINDS[i].0));
        })
        .grow(1.0)
        .min_w(0.0);
    let refresh = ui::base::panel_styles::panel_icon_button(t, glyph("RefreshCw"), "Refresh")
        .key("github-refresh")
        .opacity(if refreshing { 0.5 } else { 1.0 });
    let refresh = if refreshing { refresh } else { refresh.on_press(|s: &mut State| s.github_refresh_list()) };
    let word = |v: &dyn erased_word::Word| v.word();
    let state_cwd = cwd.to_owned();
    let state_chip = choice_menu(
        s, t, STATE_MENU, "State", "CircleDot",
        STATES.iter().map(|(v, l)| (word(v), (*l).to_owned())).collect(),
        &word(&filter.state), "open", touch,
        move |s: &mut State, v: &str| {
            let state = STATES.iter().find(|(k, _)| word(k) == v).map(|(k, _)| *k);
            s.github_set_filter(&state_cwd, FilterPatch { state, ..FilterPatch::default() });
        },
        layer,
    );
    let count = filter.labels.len();
    let label_name = if count > 0 { format!("Labels: {}", filter.labels.join(", ")) } else { "Labels".into() };
    let labels_cwd = cwd.to_owned();
    let labels_initial = filter.labels.clone();
    let labels_chip = ui::github::filter_chip(t, &label_name, "Labels", Some("Tag"), count > 0, count, touch)
        .key("github-labels-chip")
        .on_press(move |s: &mut State| s.github_open_picker(PickerFor::LabelFilter(labels_cwd.clone()), labels_initial.clone()));
    let labels_chip = picker_anchor(s, t, labels_chip, &PickerFor::LabelFilter(cwd.to_owned()), touch, layer);
    let meta = s.github.cache.meta(cwd, filter.repo).and_then(|m| m.data.as_ref());
    let people = meta.map(|m| m.assignable_users.clone()).unwrap_or_default();
    let viewer = meta.map(|m| m.viewer.as_str());
    let who_cwd = cwd.to_owned();
    let assignee_chip = choice_menu(
        s, t, ASSIGNEE_MENU, "Assignee", "User",
        logic::involvement_options(filter, &people, viewer),
        &logic::involvement(filter), "anyone", touch,
        move |s: &mut State, v: &str| s.github_set_filter(&who_cwd, logic::involvement_patch(v)),
        layer,
    );
    let sort_cwd = cwd.to_owned();
    let sort_chip = choice_menu(
        s, t, SORT_MENU, "Sort", "ArrowDown",
        SORTS.iter().map(|(v, l)| (word(v), (*l).to_owned())).collect(),
        &word(&filter.sort), "updated", touch,
        move |s: &mut State, v: &str| {
            let sort = SORTS.iter().find(|(k, _)| word(k) == v).map(|(k, _)| *k);
            s.github_set_filter(&sort_cwd, FilterPatch { sort, ..FilterPatch::default() });
        },
        layer,
    );
    let chips = row().key("github-chips").w_full().gap(6.0).items_center().child(state_chip).child(labels_chip).child(assignee_chip).child(sort_chip);
    let mut out = col()
        .key("github-filters")
        .w_full()
        .gap(8.0)
        .px(8.0)
        .pt(4.0)
        .pb(8.0)
        .child(search)
        .child(row().w_full().gap(4.0).items_center().child(segment).child(refresh))
        .child(el().key("github-chips-scroll").w_full().scroll_x().child(chips));
    if count > 0 {
        out = out.child(active_labels(s, t, cwd, filter));
    }
    out
}

/// The active label filters as removable chips, and Clear all.
fn active_labels(s: &State, t: &'static Tokens, cwd: &str, filter: &ListFilter) -> El {
    let colors = s.github.cache.meta(cwd, filter.repo).and_then(|m| m.data.as_ref()).map(|m| m.labels.clone()).unwrap_or_default();
    let mut group = row().key("github-active-labels").role("group").label("Active label filters").w_full().gap(6.0).items_center().wrap();
    let prefix = match filter.label_mode {
        LabelMode::Any => None,
        LabelMode::All => Some("All of"),
        LabelMode::Exclude => Some("Without"),
    };
    if let Some(prefix) = prefix {
        group = group.child(text(prefix).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground));
    }
    for name in &filter.labels {
        let color = colors.iter().find(|l| &l.name == name).map(|l| l.color.clone()).unwrap_or_default();
        let label = model::github::Label { name: name.clone(), color };
        let rest: Vec<String> = filter.labels.iter().filter(|l| *l != name).cloned().collect();
        let at = cwd.to_owned();
        group = group.child(
            row().gap(2.0).items_center().child(ui::github::label_chip(t, &label, true)).child(
                ui::git::action(t, "X", &format!("Remove {name}"), 12.0, false, false)
                    .key(format!("github-remove-label-{name}"))
                    .on_press(move |s: &mut State| s.github_set_filter(&at, FilterPatch { labels: Some(rest.clone()), ..FilterPatch::default() })),
            ),
        );
    }
    let at = cwd.to_owned();
    group.child(
        row()
            .key("github-clear-labels")
            .role("button")
            .label("Clear all")
            .px(4.0)
            .radius(6.0)
            .child(text("Clear all").font(sans(12.0, 500.0, 16.0)).color(t.colors.primary))
            .on_press(move |s: &mut State| {
                s.github_set_filter(&at, FilterPatch { labels: Some(Vec::new()), label_mode: Some(LabelMode::Any), ..FilterPatch::default() })
            }),
    )
}

mod erased_word {
    /// The serde name of a unit enum value (`"open"`).
    pub trait Word {
        fn word(&self) -> String;
    }
    impl<T: serde::Serialize> Word for T {
        fn word(&self) -> String {
            serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
        }
    }
}

impl State {
    /// The assignee menu needs the repository's people.
    pub(crate) fn github_meta_for_filter(&mut self) {
        let Some(cwd) = self.github_cwd() else { return };
        let repo = self.github.cache.filter(&cwd).repo;
        let meta = self.github.cache.meta(&cwd, repo);
        if meta.is_none_or(|m| m.data.is_none() && m.status != Status::Loading) {
            self.github_step(model::github::Event::LoadRepoMeta(cwd, repo, None));
        }
    }
}

/// The trigger of a picker, with the picker anchored under it on desktop when it is the one
/// open. On the phone the picker is a sheet ([`picker_sheet`]).
pub fn picker_anchor(s: &State, t: &'static Tokens, trigger: El, of: &PickerFor, touch: bool, layer: &mut Vec<El>) -> El {
    let open = !touch && s.overlays.is_open(PICKER) && s.github.picker.as_ref().is_some_and(|p| &p.of == of);
    if !open {
        return trigger;
    }
    Popover::new(PICKER, trigger)
        .label(picker_title(of))
        .placement(Placement::BottomStart)
        .width(320.0)
        .padding(0.0)
        .open(true)
        .on_close(|s: &mut State| s.github_close_picker())
        .child(picker_body(s, t, false))
        .build(t, layer)
}

fn picker_title(of: &PickerFor) -> &'static str {
    match of {
        PickerFor::LabelFilter(_) => "Filter by label",
        PickerFor::Labels(_) => "Labels",
        PickerFor::Assignees(_) => "Assignees",
    }
}

/// The phone's picker sheet.
pub fn picker_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let Some(picker) = &s.github.picker else { return };
    let title = if picker.create.is_some() { "New label" } else { picker_title(&picker.of) };
    crate::phone::sheet(PICKER, title.to_owned(), s).child(picker_body(s, t, true)).build(t, window, layer);
}

/// `PickerBody`: search, the match mode of the label filter, the options with checks, Create
/// label, and Cancel and the apply button.
fn picker_body(s: &State, t: &'static Tokens, touch: bool) -> El {
    let c = &t.colors;
    let Some(picker) = &s.github.picker else { return el() };
    if picker.create.is_some() {
        return create_form(s, t, touch);
    }
    let (cwd, repo) = match &picker.of {
        PickerFor::LabelFilter(cwd) => (cwd.clone(), s.github.cache.filter(cwd).repo),
        PickerFor::Labels(at) | PickerFor::Assignees(at) => (at.cwd.clone(), at.repo),
    };
    let meta = s.github.cache.meta(&cwd, repo);
    let data = meta.and_then(|m| m.data.as_ref());
    let status = match (data, meta.map(|m| m.status)) {
        (Some(_), _) => "ready",
        (None, Some(Status::Error)) => "error",
        _ => "loading",
    };
    let labels = !matches!(picker.of, PickerFor::Assignees(_));
    let options: Vec<(String, Option<String>, El)> = match (&picker.of, data) {
        (PickerFor::Assignees(_), Some(m)) => m.assignable_users.iter().map(|u| (u.clone(), None, ui::github::avatar(t, u, true))).collect(),
        (_, Some(m)) => m.labels.iter().map(|l| (l.name.clone(), l.description.clone(), ui::github::label_dot(t, &l.color))).collect(),
        _ => Vec::new(),
    };
    // Selected-at-open first, then the rest in GitHub's order; values GitHub no longer lists
    // stay pickable so they can be removed.
    let known: Vec<&str> = options.iter().map(|(v, _, _)| v.as_str()).collect();
    let mut ordered: Vec<(String, Option<String>, Option<El>)> =
        picker.opened.iter().filter(|v| !known.contains(&v.as_str())).map(|v| (v.clone(), None, None)).collect();
    ordered.extend(options.into_iter().map(|(v, d, e)| (v, d, Some(e))));
    ordered.sort_by_key(|(v, _, _)| !picker.opened.contains(v));
    let needle = picker.query.text.trim().to_lowercase();
    let visible: Vec<_> = ordered.into_iter().filter(|(v, _, _)| needle.is_empty() || v.to_lowercase().contains(&needle)).collect();
    let exact = data.is_some_and(|m| m.labels.iter().any(|l| l.name.to_lowercase() == needle));
    let offer_create = matches!(picker.of, PickerFor::Labels(_)) && !needle.is_empty() && !exact && status == "ready";
    let search_label = match picker.of {
        PickerFor::LabelFilter(_) => "Find a label",
        PickerFor::Labels(_) => "Find or create a label",
        PickerFor::Assignees(_) => "Find a person",
    };
    let mut body = col().key("github-picker-body").w_full().gap(6.0);
    let mut head = row().w_full().px(12.0).pt(10.0).justify_between().items_center();
    if !touch {
        head = head.child(text(picker_title(&picker.of)).font(sans(12.0, 500.0, 16.0)).color(c.foreground));
    }
    if matches!(picker.of, PickerFor::LabelFilter(_)) {
        let empty = picker.selected.is_empty();
        head = head.child(
            row()
                .key("github-picker-clear")
                .role("button")
                .label("Clear")
                .px(6.0)
                .child(text("Clear").font(sans(12.0, 500.0, 16.0)).color(if empty { c.muted_foreground } else { c.primary }))
                .on_press(|s: &mut State| {
                    if let Some(p) = s.github.picker.as_mut() {
                        p.selected.clear();
                    }
                }),
        );
    }
    body = body.child(head);
    body = body.child(
        el().w_full().px(8.0).child(
            ui::base::panel_styles::panel_field(t, &picker.query, search_label, touch)
                .key("github-picker-search")
                .w_full()
                .h(if touch { 44.0 } else { 32.0 })
                .autofocus()
                .on_edit(|s: &mut State| &mut s.github.picker.as_mut().expect("picker open").query),
        ),
    );
    if let PickerFor::LabelFilter(_) = picker.of {
        let modes: Vec<&str> = MODES.iter().map(|(_, l)| *l).collect();
        let active = MODES.iter().position(|(m, _)| *m == picker.mode).unwrap_or(0);
        body = body.child(el().w_full().px(8.0).child(ui::base::panel_styles::Segmented::new("Match", &modes, active).build(t, "github-picker-mode", false, |s: &mut State, i: usize| {
            if let Some(p) = s.github.picker.as_mut() {
                p.mode = MODES[i].0;
            }
        })));
    }
    match status {
        "loading" => body = body.child(skeleton_list(t)),
        "error" => {
            let error = meta.and_then(|m| m.error.clone()).map_or(".".into(), |e| format!(": {e}"));
            body = body.child(text(format!("Couldn't load from GitHub{error}")).font(sans(12.0, 400.0, 16.0)).color(c.destructive).px(12.0).role("alert"));
        }
        _ => {}
    }
    if status != "loading" {
        let mut list = col().key("github-picker-list").w_full().px(4.0);
        for (value, description, leading) in visible.iter() {
            let checked = picker.selected.contains(value);
            let pick = value.clone();
            let mut r = row()
                .key(format!("github-pick-{value}"))
                .role("menuitemcheckbox")
                .label(value.clone())
                .checked(checked)
                .w_full()
                .min_h(if touch { 44.0 } else { 32.0 })
                .px(8.0)
                .gap(8.0)
                .items_center()
                .radius(8.0)
                .hover(c.accent)
                .child(
                    row()
                        .w(16.0)
                        .h(16.0)
                        .center()
                        .radius(4.0)
                        .border(1.0, if checked { c.primary } else { c.border })
                        .fill(if checked { c.primary } else { rune::Color::TRANSPARENT })
                        .a11y_hidden()
                        .child(if checked { icon(glyph("Check"), 12.0).color(c.primary_foreground) } else { el() }),
                );
            if let Some(leading) = leading {
                r = r.child(leading.clone());
            }
            r = r.child(text(value.clone()).font(sans(12.0, 400.0, 16.0)).color(c.foreground).single_line().truncate().grow(1.0).min_w(0.0));
            if let Some(d) = description {
                r = r.child(text(d.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().max_w(130.0));
            }
            list = list.child(r.on_press(move |s: &mut State| s.github_toggle_pick(&pick)));
        }
        if offer_create {
            let name = picker.query.text.trim().to_owned();
            list = list.child(
                row()
                    .key("github-picker-create")
                    .role("menuitem")
                    .label(format!("Create label \u{201c}{name}\u{201d}"))
                    .w_full()
                    .min_h(if touch { 44.0 } else { 32.0 })
                    .px(8.0)
                    .gap(8.0)
                    .items_center()
                    .radius(8.0)
                    .hover(c.accent)
                    .child(icon(glyph("Plus"), 14.0).color(c.foreground))
                    .child(text(format!("Create label \u{201c}{name}\u{201d}")).font(sans(12.0, 400.0, 16.0)).color(c.foreground).single_line().truncate())
                    .on_press(move |s: &mut State| {
                        if let Some(p) = s.github.picker.as_mut() {
                            p.create = Some((rune::TextEdit::new(&name), LABEL_SWATCHES[0], rune::TextEdit::default()));
                        }
                    }),
            );
        }
        if visible.is_empty() && !offer_create && status == "ready" {
            list = list.child(note(t, "No matches.").py(16.0));
        }
        body = body.child(ui::base::scroll_area::scroll_area(t, "github-picker-scroll", list).max_h(288.0));
    }
    let apply_label = match &picker.of {
        PickerFor::LabelFilter(_) => logic::filter_apply_label(picker.selected.len()),
        PickerFor::Labels(at) => {
            let now: Vec<String> = s.github.cache.item(at).and_then(|r| r.data.as_ref()).map(|d| d.item.labels.iter().map(|l| l.name.clone()).collect()).unwrap_or_default();
            logic::done_label(&picker.selected, &now, "label")
        }
        PickerFor::Assignees(at) => {
            let now = s.github.cache.item(at).and_then(|r| r.data.as_ref()).map(|d| d.item.assignees.clone()).unwrap_or_default();
            logic::done_label(&picker.selected, &now, "assignee")
        }
    };
    let gated = labels && false;
    let apply = Button::new(if picker.applying { "Saving\u{2026}".to_owned() } else { apply_label })
        .size(if touch { ButtonSize::Touch } else { ButtonSize::Sm })
        .disabled(picker.applying || gated || status != "ready")
        .build(t)
        .key("github-picker-apply")
        .on_press(|s: &mut State| s.github_apply_picker());
    let cancel = Button::new("Cancel")
        .variant(if touch { ButtonVariant::Outline } else { ButtonVariant::Ghost })
        .size(if touch { ButtonSize::Touch } else { ButtonSize::Sm })
        .build(t)
        .key("github-picker-cancel")
        .on_press(|s: &mut State| s.github_close_picker());
    let mut foot = row().w_full().px(8.0).py(8.0).gap(8.0).items_center().border_sides([0.5, 0.0, 0.0, 0.0], c.border);
    if !matches!(picker.of, PickerFor::LabelFilter(_)) && !touch {
        foot = foot.child(text("Applied on GitHub when you tap Done").font(sans(11.0, 400.0, 14.0)).color(c.muted_foreground).single_line().truncate().grow(1.0).min_w(0.0));
    } else {
        foot = foot.child(el().grow(1.0));
    }
    body.child(foot.child(cancel).child(apply))
}

/// `CreateLabelForm`: name, one of GitHub's colours, an optional description and a preview.
fn create_form(s: &State, t: &'static Tokens, touch: bool) -> El {
    let c = &t.colors;
    let Some(picker) = &s.github.picker else { return el() };
    let Some((name, color, description)) = &picker.create else { return el() };
    let field_h = if touch { 44.0 } else { 32.0 };
    let caption = |line: &str| text(line.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground);
    let mut swatches = row().gap(8.0).wrap();
    for swatch in LABEL_SWATCHES {
        let ink = ui::github::label_colors(swatch, c.popover).map_or(c.muted_foreground, |(ink, _, _)| ink);
        let mut b = row()
            .key(format!("github-swatch-{swatch}"))
            .role("button")
            .label(format!("Colour #{swatch}"))
            .checked(*color == swatch)
            .w(32.0)
            .h(32.0)
            .center()
            .radius(16.0)
            .border(1.0, c.border)
            .child(el().w(20.0).h(20.0).radius(10.0).fill(ink))
            .on_press(move |s: &mut State| {
                if let Some((_, color, _)) = s.github.picker.as_mut().and_then(|p| p.create.as_mut()) {
                    *color = swatch;
                }
            });
        if *color == swatch {
            b = b.border(2.0, c.foreground);
        }
        swatches = swatches.child(b);
    }
    let preview = model::github::Label { name: if name.text.trim().is_empty() { "label".into() } else { name.text.trim().to_owned() }, color: (*color).to_owned() };
    let back = Button::new("Back")
        .variant(if touch { ButtonVariant::Outline } else { ButtonVariant::Ghost })
        .size(if touch { ButtonSize::Touch } else { ButtonSize::Sm })
        .disabled(picker.creating)
        .build(t)
        .key("github-create-back")
        .on_press(|s: &mut State| {
            if let Some(p) = s.github.picker.as_mut() {
                p.create = None;
            }
        });
    let create = Button::new(if picker.creating { "Creating\u{2026}" } else { "Create label" })
        .size(if touch { ButtonSize::Touch } else { ButtonSize::Sm })
        .disabled(picker.creating || name.text.trim().is_empty())
        .build(t)
        .key("github-create-label")
        .on_press(|s: &mut State| s.github_create_label());
    let mut form = col().key("github-create-form").w_full().gap(12.0).padding(if touch { 0.0 } else { 12.0 });
    if !touch {
        form = form.child(text("New label").font(sans(12.0, 500.0, 16.0)).color(c.foreground));
    }
    form.child(col().w_full().gap(4.0).child(caption("Name")).child(
        ui::base::panel_styles::panel_field(t, name, "Name", touch).key("github-create-name").w_full().h(field_h).on_edit(create_name),
    ))
    .child(col().w_full().gap(6.0).child(caption("Colour")).child(swatches))
    .child(col().w_full().gap(4.0).child(caption("Description (optional)")).child(
        ui::base::panel_styles::panel_field(t, description, "Description (optional)", touch).key("github-create-description").w_full().h(field_h).on_edit(create_description),
    ))
    .child(row().gap(8.0).items_center().child(caption("Preview")).child(ui::github::label_chip(t, &preview, true)))
    .child(row().w_full().justify_end().gap(8.0).child(back).child(create))
}

fn create_form_of(s: &mut State) -> &mut (rune::TextEdit, &'static str, rune::TextEdit) {
    s.github.picker.as_mut().and_then(|p| p.create.as_mut()).expect("create form open")
}

fn create_name(s: &mut State) -> &mut rune::TextEdit {
    &mut create_form_of(s).0
}

fn create_description(s: &mut State) -> &mut rune::TextEdit {
    &mut create_form_of(s).2
}
