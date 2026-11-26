//! A GitHub issue or PR as a pane tab (`GithubItemPanel.tsx`): the header with its state,
//! Refresh, Open on GitHub, the state button and the ⋯ menu (`GithubItemHeader`,
//! `GithubItemMenu`); labels and assignees with their pickers (`GithubItemMeta`); then the
//! sections: the conversation (`GithubTimeline`, `GithubMergeControls`, the comment box), the
//! checks (`GithubChecksList`) and the review threads (`GithubReviewThreads`). On the phone
//! the comment box is pinned under the page. Every write is followed by a re-read; merging asks
//! first, as React does.

use std::rc::Rc;

use model::github::{
    Check, CheckStatus, ChecksResult, CloseReason, Comment, ItemDetail, ItemKind, ItemRef, ItemState, MergeAction,
    MergeMethod, PullDetail, Resource, ReviewThread, Status,
};
use rune::{El, Tx, col, el, icon, row, text};
use theme::Tokens;
use ui::git::glyph;
use ui::markdown::{Hooks, LinkTarget};
use ui::overlay::{ConfirmDialog, Menu, MenuItem, Placement};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::github::{ITEM_MENU, MERGE_CONFIRM, MERGE_METHOD, PickerFor, STATE_BUTTON_MENU, Section, item_ref};
use crate::github_logic as logic;

fn hooks() -> Hooks<State> {
    Hooks {
        on_link: Rc::new(|s: &mut State, target: LinkTarget, _tx: &mut Tx| {
            if let LinkTarget::Url(url) = target {
                s.request_link_open(&url);
            }
        }),
        ..Hooks::default()
    }
}

fn small(t: &Tokens, line: impl Into<String>) -> El {
    text(line.into()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground)
}

/// The tab's body, for a GitHub item tab.
pub fn tab_body(s: &State, t: &'static Tokens, tab: &model::workspace::Tab, phone: bool, layer: &mut Vec<El>) -> Option<El> {
    let at = item_ref(tab)?;
    let key = model::github::item_key(&at.cwd, at.repo, at.number);
    let detail = s.github.cache.item(&at).cloned().unwrap_or_default();
    let c = &t.colors;
    let root = col().key(format!("github-item-{key}")).w_full().h_full().min_h(0.0).fill(c.background);
    let Some(data) = &detail.data else {
        let mut page = col().w_full().max_w(768.0).gap(12.0).px(16.0).py(16.0);
        if detail.status == Status::Error {
            let retry = at.clone();
            page = page
                .role("alert")
                .child(text(format!("Couldn't load #{}", at.number)).font(sans(14.0, 500.0, 20.0)).color(c.destructive))
                .children(detail.error.as_ref().map(|e| small(t, e.clone())))
                .child(Button::new("Retry").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key("github-item-retry").on_press(move |s: &mut State| s.github_refresh_item(&retry)));
        } else {
            page = page
                .child(ui::base::skeleton::skeleton(t).w(96.0).h(24.0))
                .child(ui::base::skeleton::skeleton(t).w(480.0).h(28.0))
                .child(ui::base::skeleton::skeleton(t).w_full().h(128.0));
        }
        return Some(root.child(row().w_full().justify_center().child(page)));
    };
    let ui_state = s.github.items.get(&key).cloned().unwrap_or_default();
    let checks = s.github.cache.item_checks(&at).cloned().unwrap_or_default();
    // After a push the item reports a new head before the cached checks do.
    let stale = matches!((&data.pr, &checks.data), (Some(pr), Some(c)) if c.head_sha != pr.head_sha);
    let shown_checks = if stale {
        Resource { data: None, status: if checks.status == Status::Error { Status::Error } else { Status::Loading }, ..checks.clone() }
    } else {
        checks.clone()
    };
    let refreshing = detail.status == Status::Loading;
    let mut page = col().w_full().max_w(768.0).min_w(0.0).gap(16.0).px(16.0).py(16.0);
    page = page.child(header(s, t, &at, data, refreshing, &ui_state, phone, layer));
    page = page.child(meta(s, t, &at, data, &ui_state, phone, layer));
    if detail.status == Status::Error {
        let why = if detail.error_code.as_deref() == Some("GH_RATE_LIMITED") { "GitHub rate limit reached." } else { "Couldn't refresh." };
        let line = format!("{why} Showing what was loaded {}.", logic::relative_ago(&data.fetched_at, crate::git_history::now_ms()));
        page = page.child(
            row().role("status").w_full().gap(6.0).px(10.0).py(6.0).radius(8.0).fill(c.warning.alpha(0.1))
                .child(icon(glyph("AlertTriangle"), 14.0).color(c.warning))
                .child(text(line).font(sans(12.0, 400.0, 16.0)).color(c.warning).wrap().grow(1.0).min_w(0.0)),
        );
    }
    let failed = shown_checks.data.as_ref().map_or(0, |d| d.checks.iter().filter(|c| c.status == CheckStatus::Failure).count());
    let tabs: Vec<(Section, &str, Option<String>, bool)> = match &data.pr {
        Some(pr) => vec![
            (Section::Conversation, "Conversation", Some(data.item.comment_count.to_string()), false),
            if failed > 0 {
                (Section::Checks, "Checks", Some(format!("{failed} failed")), true)
            } else {
                (Section::Checks, "Checks", shown_checks.data.as_ref().map(|d| d.checks.len().to_string()), false)
            },
            (Section::Threads, "Threads", Some(pr.review_threads.len().to_string()), false),
        ],
        None => vec![
            (Section::Conversation, "Conversation", Some(data.item.comment_count.to_string()), false),
            (Section::Linked, "Linked chats", Some("0".into()), false),
        ],
    };
    page = page.child(section_tabs(t, &at, &tabs, ui_state.section, phone));
    let section = match ui_state.section {
        Section::Conversation => {
            let mut conversation = col().w_full().min_w(0.0).gap(20.0).child(timeline(t, &key, data, &ui_state));
            if let Some(pr) = &data.pr
                && let Some(merge) = merge_box(s, t, &at, data, pr, &ui_state, refreshing, layer)
            {
                conversation = conversation.child(merge);
            }
            if !phone {
                conversation = conversation.child(composer(t, &at, None, &ui_state.comment, false, "Add a comment", "Comment", "Leave a comment (Markdown)"));
            }
            conversation
        }
        Section::Checks => checks_list(t, &at, &shown_checks, &ui_state, s),
        Section::Threads => match &data.pr {
            Some(pr) if !pr.review_threads.is_empty() => threads(t, &at, &pr.review_threads, &ui_state, phone),
            _ => text("No review threads.").font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground),
        },
        Section::Linked => text("No agent chats started from this issue yet. Start working or Ask opens one.").font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).wrap(),
    };
    page = page.child(col().key("github-section").role("tabpanel").w_full().min_w(0.0).child(section));
    let scroll = ui::base::scroll_area::scroll_area(t, &format!("github-item-scroll-{key}"), row().w_full().justify_center().child(page)).grow(1.0).min_h(0.0);
    let mut root = root.child(scroll);
    if phone && ui_state.section == Section::Conversation {
        root = root.child(
            el().w_full().px(12.0).pt(8.0).pb(8.0).border_sides([0.5, 0.0, 0.0, 0.0], c.border)
                .child(composer(t, &at, None, &ui_state.comment, true, "Add a comment", "Comment", "Comment (Markdown)")),
        );
    }
    Some(root)
}

#[allow(clippy::too_many_arguments)]
fn header(s: &State, t: &'static Tokens, at: &ItemRef, data: &ItemDetail, refreshing: bool, ui_state: &crate::github::ItemUi, phone: bool, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let item = &data.item;
    let not_planned = item.kind == ItemKind::Issue && item.state == ItemState::Closed && data.state_reason == Some(CloseReason::NotPlanned);
    let refresh_at = at.clone();
    let refresh = ui::base::panel_styles::panel_icon_button(t, glyph("RefreshCw"), "Refresh").key("github-item-refresh").opacity(if refreshing { 0.5 } else { 1.0 });
    let refresh = if refreshing { refresh } else { refresh.on_press(move |s: &mut State| s.github_refresh_item(&refresh_at)) };
    let mut actions = row().gap(4.0).items_center().child(refresh);
    if !phone {
        let url = item.url.clone();
        actions = actions.child(
            row().key("github-open-on-github").role("link").label("Open on GitHub").h(28.0).px(8.0).gap(4.0).items_center().radius(6.0).hover(c.foreground.alpha(0.03))
                .child(icon(glyph("ExternalLink"), 14.0).color(c.muted_foreground))
                .child(small(t, "Open on GitHub"))
                .on_press(move |s: &mut State| s.request_link_open(&url)),
        );
        actions = actions.child(state_button(t, at, data, ui_state, layer, s));
    }
    actions = actions.child(item_menu(s, t, at, data, ui_state, phone, layer));
    let mut head = col().w_full().min_w(0.0).gap(10.0)
        .child(
            row().w_full().gap(8.0).items_center()
                .child(ui::github::item_state_pill(t, item.kind, item.state, not_planned.then_some("Closed as not planned")))
                .child(small(t, data.repo.full_name()).single_line().truncate().min_w(0.0))
                .child(el().grow(1.0))
                .child(actions),
        )
        .child(
            rune::rich(format!("{} #{}", item.title, item.number), vec![rune::Span::new(item.title.len() + 1, item.title.len() + 2 + item.number.to_string().len()).color(c.muted_foreground)])
                .font(sans(if phone { 20.0 } else { 18.0 }, 600.0, 26.0))
                .color(c.foreground)
                .wrap()
                .w_full()
                .role("heading"),
        )
        .child({
            let now = crate::git_history::now_ms();
            row().w_full().gap(6.0).items_center().wrap()
                .child(ui::github::avatar(t, &item.author, true))
                .child(text(item.author.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                .child(small(t, format!("opened {} \u{b7} updated {}", logic::relative_ago(&data.created_at, now), logic::relative_ago(&item.updated_at, now))))
        });
    if let Some(pr) = &data.pr {
        let code = |line: &str| el().px(4.0).radius(4.0).fill(c.muted).child(text(line.to_owned()).font(mono(11.0, 400.0, 16.0)).color(c.foreground).single_line().truncate());
        let files = format!("{} {} changed", pr.changed_files, if pr.changed_files == 1 { "file" } else { "files" });
        head = head.child(
            row().w_full().gap(12.0).items_center().wrap()
                .child(row().gap(4.0).items_center().min_w(0.0)
                    .child(icon(glyph("GitBranch"), 14.0).color(c.muted_foreground))
                    .child(code(&pr.head_ref_name))
                    .child(small(t, "\u{2192}").label("into".to_owned()))
                    .child(code(&pr.base_ref_name)))
                .children(ui::base::diff_stat_counts::diff_stat_counts(t, pr.additions, pr.deletions, false, sans(12.0, 400.0, 16.0)))
                .child(small(t, files)),
        );
    }
    head
}

/// `GithubStateButton`: "Close issue ▾" with both reasons, or one Close / Reopen button.
fn state_button(t: &'static Tokens, at: &ItemRef, data: &ItemDetail, ui_state: &crate::github::ItemUi, layer: &mut Vec<El>, s: &State) -> El {
    let choices = logic::state_actions(data.item.kind, data.item.state);
    let Some(first) = choices.first().cloned() else { return el() };
    let label = if ui_state.busy { "Saving\u{2026}" } else { first.label };
    if choices.len() == 1 {
        let at = at.clone();
        return Button::new(label).variant(ButtonVariant::Outline).size(ButtonSize::Xs).icon(glyph(first.icon)).disabled(ui_state.busy).build(t).key("github-state-button")
            .on_press(move |s: &mut State| s.github_set_state(&at, first.action, first.label));
    }
    let trigger = Button::new(format!("{label} \u{25be}")).variant(ButtonVariant::Outline).size(ButtonSize::Xs).icon(glyph(first.icon)).disabled(ui_state.busy).build(t).key("github-state-button")
        .on_press({
            let at = at.clone();
            move |s: &mut State| s.github_open_for(STATE_BUTTON_MENU, &at)
        });
    let mut menu = Menu::new(STATE_BUTTON_MENU).width(208.0).placement(Placement::BottomEnd).open(s.github_is_open_for(STATE_BUTTON_MENU, at)).on_close(|s: &mut State| s.close(STATE_BUTTON_MENU));
    for choice in choices {
        let at = at.clone();
        let label = match choice.detail {
            Some(detail) => format!("{}  {detail}", choice.label),
            None => choice.label.to_owned(),
        };
        menu = menu.item(MenuItem::new(label).icon(glyph(choice.icon)).on_select(move |s: &mut State| s.github_set_state(&at, choice.action, choice.label)));
    }
    menu.build(t, layer, trigger)
}

/// The viewer's self-assign action: `assign`, `unassign`, loading, or none when GitHub would
/// not let the viewer be assigned here.
fn self_action(s: &State, at: &ItemRef, data: &ItemDetail) -> Option<(String, bool, bool)> {
    let meta = s.github.cache.meta(&at.cwd, at.repo);
    match meta.and_then(|m| m.data.as_ref()) {
        Some(m) => {
            let assigned = data.item.assignees.contains(&m.viewer);
            (assigned || m.assignable_users.contains(&m.viewer)).then(|| (m.viewer.clone(), assigned, false))
        }
        None if meta.is_some_and(|m| m.status == Status::Error) => None,
        None => Some((String::new(), false, true)),
    }
}

/// `GithubItemMenu`: edit labels and assignees, assign yourself, open or copy the link, close
/// and reopen.
fn item_menu(s: &State, t: &'static Tokens, at: &ItemRef, data: &ItemDetail, ui_state: &crate::github::ItemUi, phone: bool, layer: &mut Vec<El>) -> El {
    let noun = ui::github::kind_noun(data.item.kind);
    let trigger = ui::base::panel_styles::panel_icon_button(t, glyph("MoreHorizontal"), format!("{noun} actions"))
        .key("github-item-menu-button")
        .w(if phone { 44.0 } else { 28.0 })
        .h(if phone { 44.0 } else { 28.0 })
        .on_press({
            let at = at.clone();
            move |s: &mut State| s.github_open_for(ITEM_MENU, &at)
        });
    if phone {
        return trigger;
    }
    let labels: Vec<String> = data.item.labels.iter().map(|l| l.name.clone()).collect();
    let (labels_at, assignees_at) = (at.clone(), at.clone());
    let assignees = data.item.assignees.clone();
    let mut menu = Menu::new(ITEM_MENU)
        .name(format!("{noun} #{}", data.item.number))
        .width(208.0)
        .placement(Placement::BottomEnd)
        .open(s.github_is_open_for(ITEM_MENU, at))
        .on_close(|s: &mut State| s.close(ITEM_MENU))
        .item(MenuItem::new("Edit labels").icon(glyph("Tag")).on_select(move |s: &mut State| s.github_open_picker(PickerFor::Labels(labels_at.clone()), labels.clone())));
    if let Some((viewer, assigned, loading)) = self_action(s, at, data) {
        let at = at.clone();
        menu = menu.item(
            MenuItem::new(if assigned { "Unassign me" } else { "Assign to me" })
                .icon(glyph("User"))
                .disabled(ui_state.busy || loading)
                .on_select(move |s: &mut State| s.github_toggle_self(&at, &viewer, assigned)),
        );
    }
    let (url, copy) = (data.item.url.clone(), data.item.url.clone());
    menu = menu
        .item(MenuItem::new("Edit assignees").icon(glyph("User")).on_select(move |s: &mut State| s.github_open_picker(PickerFor::Assignees(assignees_at.clone()), assignees.clone())))
        .separator()
        .item(MenuItem::new("Open on GitHub").icon(glyph("ExternalLink")).on_select(move |s: &mut State| {
            s.close(ITEM_MENU);
            s.request_link_open(&url);
        }))
        .item(MenuItem::new("Copy link").icon(glyph("Copy")).on_select_tx(move |s: &mut State, tx: &mut Tx| {
            s.close(ITEM_MENU);
            tx.copy(copy.clone());
            s.toasts.push(s.now, ui::overlay::ToastKind::Success, "Link copied", None);
        }));
    let choices = logic::state_actions(data.item.kind, data.item.state);
    if !choices.is_empty() {
        menu = menu.separator();
    }
    for choice in choices {
        let at = at.clone();
        let label = match choice.detail {
            Some(detail) => format!("{}  {detail}", choice.label),
            None => choice.label.to_owned(),
        };
        menu = menu.item(MenuItem::new(label).icon(glyph(choice.icon)).disabled(ui_state.busy).on_select(move |s: &mut State| s.github_set_state(&at, choice.action, choice.label)));
    }
    menu.build(t, layer, trigger)
}

/// `GithubItemMeta`'s rows: labels and assignees, each with its picker.
fn meta(s: &State, t: &'static Tokens, at: &ItemRef, data: &ItemDetail, ui_state: &crate::github::ItemUi, phone: bool, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let item = &data.item;
    let add = |label: &str, key: &str| {
        row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned())
            .w(if phone { 44.0 } else { 32.0 }).h(if phone { 44.0 } else { 32.0 }).center().radius(8.0)
            .border(1.0, c.border).hover(c.foreground.alpha(0.03))
            .child(icon(glyph("Plus"), 16.0).color(c.muted_foreground))
    };
    let names: Vec<String> = item.labels.iter().map(|l| l.name.clone()).collect();
    let labels_at = at.clone();
    let label_trigger = add("Edit labels", "github-edit-labels").on_press(move |s: &mut State| s.github_open_picker(PickerFor::Labels(labels_at.clone()), names.clone()));
    let label_trigger = crate::github_view::picker_anchor(s, t, label_trigger, &PickerFor::Labels(at.clone()), phone, layer);
    let assignees = item.assignees.clone();
    let assign_at = at.clone();
    let assign_trigger = add("Edit assignees", "github-edit-assignees").on_press(move |s: &mut State| s.github_open_picker(PickerFor::Assignees(assign_at.clone()), assignees.clone()));
    let assign_trigger = crate::github_view::picker_anchor(s, t, assign_trigger, &PickerFor::Assignees(at.clone()), phone, layer);
    let label_body = if item.labels.is_empty() {
        small(t, "None yet")
    } else {
        row().w_full().gap(6.0).wrap().children(item.labels.iter().map(|l| ui::github::label_chip(t, l, true)))
    };
    let assignee_body = if item.assignees.is_empty() {
        let mut none = row().gap(6.0).items_center().child(small(t, "No one"));
        if let Some((viewer, false, false)) = self_action(s, at, data) {
            let at = at.clone();
            none = none.child(small(t, "\u{b7}")).child(
                row().key("github-assign-yourself").role("button").label("Assign yourself").opacity(if ui_state.busy { 0.6 } else { 1.0 })
                    .child(small(t, "Assign yourself"))
                    .on_press(move |s: &mut State| s.github_toggle_self(&at, &viewer, false)),
            );
        }
        none
    } else {
        col().gap(6.0).children(item.assignees.iter().map(|login| {
            row().gap(8.0).items_center().child(ui::github::avatar(t, login, true)).child(text(login.clone()).font(sans(12.0, 400.0, 16.0)).color(c.foreground))
        }))
    };
    let line = |term: &str, body: El, trigger: El| {
        row().w_full().min_h(48.0).py(6.0).gap(12.0).items_center()
            .child(text(term.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).w(80.0))
            .child(el().grow(1.0).min_w(0.0).child(body))
            .child(trigger)
    };
    col().key("github-meta").w_full().border_sides([0.5, 0.0, 0.5, 0.0], c.border)
        .child(line("Labels", label_body, label_trigger))
        .child(el().w_full().h(0.5).fill(c.border))
        .child(line("Assignees", assignee_body, assign_trigger))
}

fn section_tabs(t: &Tokens, at: &ItemRef, tabs: &[(Section, &str, Option<String>, bool)], active: Section, phone: bool) -> El {
    let c = &t.colors;
    let mut bar = row().key("github-sections").role("tablist").label("Sections").w_full().border_sides([0.0, 0.0, 1.0, 0.0], c.border);
    for (section, label, count, alert) in tabs {
        let selected = *section == active;
        let mut tab = row()
            .key(format!("github-section-{}", label.to_lowercase().replace(' ', "-")))
            .role("tab")
            .label(label.to_string())
            .checked(selected)
            .h(if phone { 48.0 } else { 40.0 })
            .px(12.0)
            .gap(6.0)
            .items_center()
            .border_sides([0.0, 0.0, 2.0, 0.0], if selected { c.foreground } else { rune::Color::TRANSPARENT })
            .child(text(label.to_string()).font(sans(14.0, if selected { 500.0 } else { 400.0 }, 20.0)).color(if selected { c.foreground } else { c.muted_foreground }));
        if let Some(count) = count {
            let (fill, ink) = if *alert { (c.destructive.alpha(0.2), c.destructive) } else { (c.muted, c.muted_foreground) };
            tab = tab.child(el().px(6.0).radius(8.0).fill(fill).child(text(count.clone()).font(sans(12.0, 400.0, 16.0)).color(ink)));
        }
        let (at, section) = (at.clone(), *section);
        bar = bar.child(tab.on_press(move |s: &mut State| s.github_item_ui(&at).section = section));
    }
    bar
}

/// One timeline entry: a 32 px rail glyph, then the content.
fn timeline_row(t: &Tokens, rail: El, body: El, last: bool) -> El {
    let mut lane = col().w(32.0).items_center().child(rail);
    if !last {
        lane = lane.child(el().w(1.0).grow(1.0).fill(t.colors.border));
    }
    row().role("listitem").w_full().min_w(0.0).gap(12.0).child(lane).child(col().grow(1.0).min_w(0.0).pt(6.0).pb(20.0).child(body))
}

fn markdown(t: &'static Tokens, key: &str, body: &str, empty: &str) -> El {
    if body.trim().is_empty() {
        return text(empty.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground);
    }
    ui::markdown::prose(t, key, body, false, &hooks())
}

/// `GithubTimeline`: the description as the opening post, then comments and events in time
/// order. Bot comments start folded to one line.
fn timeline(t: &'static Tokens, key: &str, data: &ItemDetail, ui_state: &crate::github::ItemUi) -> El {
    let c = &t.colors;
    let now = crate::git_history::now_ms();
    let when = |iso: &str| small(t, crate::git_history::relative_time(iso, now));
    let item = &data.item;
    let entries = logic::timeline_entries(data);
    let mut list = col().key("github-timeline").role("list").label("Conversation").w_full().min_w(0.0);
    let opening = col().w_full().min_w(0.0)
        .child(row().gap(6.0).items_center().child(text(item.author.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground)).child(small(t, "opened \u{b7}")).child(when(&data.created_at)))
        .child(col().role("region").label("Description").w_full().pt(4.0).child(markdown(t, &format!("{key}-body"), &data.body, "No description provided.")));
    list = list.child(timeline_row(t, ui::github::avatar(t, &item.author, false), opening, entries.is_empty()));
    let count = entries.len();
    for (n, entry) in entries.into_iter().enumerate() {
        let last = n + 1 == count;
        list = list.child(match entry {
            logic::Entry::Comment(comment) if comment.is_bot => bot_comment(t, comment, ui_state.shown.contains(&comment.id), last, key),
            logic::Entry::Comment(comment) => timeline_row(
                t,
                ui::github::avatar(t, &comment.author, false),
                col().role("article").label(format!("Comment by {}", comment.author)).w_full().min_w(0.0)
                    .child(row().gap(6.0).items_center().child(text(comment.author.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground)).child(small(t, "\u{b7}")).child(when(&comment.created_at)))
                    .child(el().w_full().pt(4.0).child(markdown(t, &comment.id, &comment.body, "No content."))),
                last,
            ),
            logic::Entry::Event(event) => {
                let (glyph_name, verb) = logic::event_words(event);
                let own = event.assignee.as_deref() == Some(event.actor.as_str());
                let actor = if event.actor.is_empty() { "Someone".to_owned() } else { event.actor.clone() };
                let mut line = row().gap(6.0).items_center().wrap()
                    .child(text(actor).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                    .child(small(t, verb));
                if let Some(label) = &event.label {
                    line = line.child(ui::github::label_chip(t, label, false));
                }
                if let (Some(assignee), false) = (&event.assignee, own) {
                    line = line.child(text(assignee.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground));
                }
                line = line.child(small(t, "\u{b7}")).child(when(&event.created_at));
                timeline_row(t, el().pt(4.0).child(icon(glyph(glyph_name), 16.0).color(c.muted_foreground)), line, last)
            }
        });
    }
    list
}

fn bot_comment(t: &'static Tokens, comment: &Comment, open: bool, last: bool, key: &str) -> El {
    let c = &t.colors;
    let id = comment.id.clone();
    let item_key = key.to_owned();
    let toggle = row()
        .key(format!("github-bot-{}", comment.id))
        .role("button")
        .label(format!("{} comment by {}", if open { "Hide" } else { "Show" }, comment.author))
        .checked(open)
        .px(4.0)
        .child(text(if open { "Hide" } else { "Show" }).font(sans(12.0, 500.0, 16.0)).color(c.primary))
        .on_press(move |s: &mut State| {
            if let Some(item) = s.github.items.get_mut(&item_key)
                && !item.shown.remove(&id)
            {
                item.shown.insert(id.clone());
            }
        });
    let summary = if open { crate::git_history::relative_time(&comment.created_at, crate::git_history::now_ms()) } else { logic::plain_preview(&comment.body) };
    let mut card = col().role("article").label(format!("Comment by {} (bot)", comment.author)).w_full().min_w(0.0).radius(12.0).border(1.0, c.border)
        .child(
            row().w_full().min_h(40.0).px(12.0).gap(8.0).items_center()
                .child(text(comment.author.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).single_line())
                .child(el().px(6.0).radius(8.0).fill(c.muted).child(text("bot").font(sans(10.0, 400.0, 14.0)).color(c.muted_foreground)))
                .child(small(t, summary).single_line().truncate().grow(1.0).min_w(0.0))
                .child(toggle),
        );
    if open {
        card = card.child(el().w_full().px(12.0).py(8.0).border_sides([0.5, 0.0, 0.0, 0.0], c.border).child(markdown(t, &comment.id, &comment.body, "No content.")));
    }
    let rail = row().w(32.0).h(32.0).center().radius(16.0).fill(c.muted).child(icon(glyph("Bot"), 16.0).color(c.muted_foreground));
    timeline_row(t, rail, card, last)
}

/// `GithubMergeControls`: merge only a PR GitHub reports CLEAN or HAS_HOOKS, after a confirm.
#[allow(clippy::too_many_arguments)]
fn merge_box(s: &State, t: &'static Tokens, at: &ItemRef, data: &ItemDetail, pr: &PullDetail, ui_state: &crate::github::ItemUi, refreshing: bool, layer: &mut Vec<El>) -> Option<El> {
    let c = &t.colors;
    let rules = logic::merge_box(data.item.state, pr, ui_state.method)?;
    let busy = ui_state.acting || refreshing;
    let (method_label, verb) = logic::method_words(ui_state.method);
    let trigger = row().key("github-merge-method").role("combobox").label("Merge method").h(36.0).w(176.0).px(12.0).items_center().justify_between().radius(6.0).border(1.0, c.input)
        .opacity(if busy { 0.5 } else { 1.0 })
        .child(text(method_label).font(sans(12.0, 400.0, 16.0)).color(c.foreground))
        .child(icon(glyph("ChevronDown"), 14.0).color(c.muted_foreground));
    let trigger = if busy { trigger } else { trigger.on_press({
            let at = at.clone();
            move |s: &mut State| s.github_open_for(MERGE_METHOD, &at)
        }) };
    let mut methods = Menu::new(MERGE_METHOD).width(176.0).placement(Placement::BottomStart).open(s.github_is_open_for(MERGE_METHOD, at)).on_close(|s: &mut State| s.close(MERGE_METHOD));
    for method in [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase] {
        let at = at.clone();
        methods = methods.item(MenuItem::new(logic::method_words(method).0).checked(method == ui_state.method).on_select(move |s: &mut State| {
            s.close(MERGE_METHOD);
            s.github_item_ui(&at).method = method;
        }));
    }
    let merge = Button::new(verb).size(ButtonSize::Sm).disabled(busy || !rules.can_merge).build(t).key("github-merge")
        .on_press({
            let at = at.clone();
            move |s: &mut State| s.github_open_for(MERGE_CONFIRM, &at)
        });
    let mut controls = row().gap(8.0).items_center().wrap().child(methods.build(t, layer, trigger)).child(merge);
    if let Some(action) = rules.auto_merge {
        let label = if action == MergeAction::DisableAutoMerge { "Disable auto-merge" } else { "Enable auto-merge" };
        let at = at.clone();
        controls = controls.child(Button::new(label).variant(ButtonVariant::Outline).size(ButtonSize::Sm).disabled(busy).build(t).key("github-auto-merge")
            .on_press(move |s: &mut State| s.github_merge(&at, action)));
    }
    let mut section = col().key("github-merge-box").role("region").label("Merge").w_full().gap(8.0).padding(12.0).radius(8.0).border(1.0, c.border).fill(c.card)
        .child(text("Merge").font(sans(14.0, 500.0, 20.0)).color(c.foreground));
    if let Some(reason) = rules.reason {
        section = section.child(small(t, reason));
    }
    if pr.auto_merge_enabled {
        section = section.child(text("Auto-merge is on: GitHub merges this when its requirements pass.").font(sans(12.0, 400.0, 16.0)).color(c.foreground).wrap());
    }
    if s.github_is_open_for(MERGE_CONFIRM, at) {
        let (at, method) = (at.clone(), ui_state.method);
        ConfirmDialog::new(MERGE_CONFIRM, format!("{verb} pull request #{}?", data.item.number), format!("{verb} {} into {} on GitHub.", pr.head_ref_name, pr.base_ref_name))
            .confirm_label(verb)
            .loading(ui_state.acting)
            .open(true)
            .on_cancel(|s: &mut State| s.close(MERGE_CONFIRM))
            .on_confirm(move |s: &mut State| s.github_merge(&at, MergeAction::Merge { method }))
            .build(t, layer);
    }
    Some(section.child(controls))
}

/// `GithubCommentComposer`: Markdown that posts to GitHub. Cmd/Ctrl+Enter posts; the text
/// clears only once GitHub accepts it.
#[allow(clippy::too_many_arguments)]
fn composer(t: &Tokens, at: &ItemRef, thread: Option<&str>, draft: &crate::github::Draft, pinned: bool, label: &str, submit: &str, placeholder: &str) -> El {
    let c = &t.colors;
    let can = !draft.posting && !draft.body.text.trim().is_empty();
    let slot = thread.map_or_else(|| "conversation".to_owned(), str::to_owned);
    let edit_at = at.clone();
    let edit_thread = thread.map(str::to_owned);
    let field = rune::text_input(&draft.body, sans(if pinned { 16.0 } else { 14.0 }, 400.0, 20.0))
        .label(label.to_owned())
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .key(format!("github-comment-{slot}"))
        .w_full()
        .min_h(if pinned { 44.0 } else { 72.0 })
        .px(12.0)
        .py(8.0)
        .radius(if pinned { 22.0 } else { 6.0 })
        .border(1.0, c.input)
        .focus_ring(2.0, -1.0, c.focus_ring);
    // Read-only while posting, as React's `readOnly={posting}`: text typed then would be
    // cleared with the sent text.
    let field = if draft.posting {
        field
    } else {
        field.on_edit(move |s: &mut State| {
            let item = s.github_item_ui(&edit_at);
            let draft = match &edit_thread {
                Some(id) => item.replies.entry(id.clone()).or_default(),
                None => &mut item.comment,
            };
            &mut draft.body
        })
    };
    let post_at = at.clone();
    let post_thread = thread.map(str::to_owned);
    let post = move |s: &mut State| s.github_post_comment(&post_at, post_thread.clone());
    let mut out = col().w_full().min_w(0.0).gap(8.0);
    if let Some(error) = &draft.error {
        out = out.child(text(error.clone()).font(sans(12.0, 400.0, 16.0)).color(c.destructive).role("alert").wrap());
    }
    if pinned {
        let send = Button::icon_only(glyph("ArrowUp"), if draft.posting { "Posting\u{2026}" } else { submit }, ButtonSize::Icon).disabled(!can).build(t)
            .key(format!("github-post-{slot}")).w(44.0).h(44.0).radius(22.0).on_press(post);
        return out.child(row().w_full().gap(8.0).items_end().child(el().grow(1.0).min_w(0.0).child(field)).child(send));
    }
    let mut buttons = row().w_full().justify_end().gap(8.0);
    if let Some(id) = thread {
        let (at, id) = (at.clone(), id.to_owned());
        buttons = buttons.child(Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).disabled(draft.posting).build(t).key(format!("github-cancel-{slot}"))
            .on_press(move |s: &mut State| {
                s.github_item_ui(&at).replies.remove(&id);
            }));
    }
    buttons = buttons.child(Button::new(if draft.posting { "Posting\u{2026}" } else { submit }).variant(ButtonVariant::Secondary).size(ButtonSize::Sm).disabled(!can).build(t)
        .key(format!("github-post-{slot}")).on_press(post));
    out.child(field).child(buttons)
}

/// `GithubChecksList`: a count per status, the checks that need a look, the rest folded.
fn checks_list(t: &'static Tokens, at: &ItemRef, checks: &Resource<ChecksResult>, ui_state: &crate::github::ItemUi, s: &State) -> El {
    let c = &t.colors;
    let all: Vec<Check> = checks.data.as_ref().map(|d| d.checks.clone()).unwrap_or_default();
    let rows = logic::sorted_checks(&all);
    let mut out = col().key("github-checks").role("region").label("Checks").w_full().gap(8.0);
    if !rows.is_empty() {
        let mut tally = row().gap(12.0).wrap();
        for (status, n) in logic::counts(&rows) {
            let (word, name, tone) = ui::github::check_look(status);
            tally = tally.child(row().gap(4.0).items_center().child(icon(glyph(name), 14.0).color(tone.ink(t))).child(text(format!("{n} {}", word.to_lowercase())).font(sans(12.0, 400.0, 16.0)).color(tone.ink(t))));
        }
        out = out.child(tally);
    }
    if checks.status == Status::Error {
        let what = if checks.data.is_some() { "refresh" } else { "load" };
        let tail = checks.error.as_ref().map_or(".".to_owned(), |e| format!(": {e}"));
        out = out.child(text(format!("Couldn't {what} checks{tail}")).font(sans(12.0, 400.0, 16.0)).color(c.destructive));
    }
    if checks.data.is_none() && checks.status != Status::Error {
        out = out.child(ui::base::skeleton::skeleton(t).w_full().h(32.0));
    }
    if checks.data.is_some() && rows.is_empty() {
        out = out.child(small(t, "This pull request has no checks."));
    }
    if rows.is_empty() {
        return out;
    }
    let (attention, folded): (Vec<&Check>, Vec<&Check>) = rows.iter().partition(|c| logic::needs_attention(c.status));
    let shown = if ui_state.all_checks { rows.clone() } else { attention };
    let mut card = col().w_full().radius(12.0).border(1.0, c.border);
    for check in shown {
        card = card.child(check_row(t, at, check, ui_state, s));
    }
    if !folded.is_empty() {
        let words = logic::folded_words(&folded);
        let label = if ui_state.all_checks { format!("Hide {words}") } else { format!("Show {} {words}", folded.len()) };
        let toggle_at = at.clone();
        card = card.child(
            row().key("github-checks-fold").role("button").label(label.clone()).checked(ui_state.all_checks).w_full().min_h(40.0).px(12.0).gap(6.0).center()
                .border_sides([0.5, 0.0, 0.0, 0.0], c.border).hover(c.foreground.alpha(0.03))
                .child(small(t, label)).child(icon(glyph("ChevronDown"), 14.0).color(c.muted_foreground))
                .on_press(move |s: &mut State| {
                    let item = s.github_item_ui(&toggle_at);
                    item.all_checks = !item.all_checks;
                }),
        );
    }
    out.child(card)
}

fn check_row(t: &'static Tokens, at: &ItemRef, check: &Check, ui_state: &crate::github::ItemUi, s: &State) -> El {
    let c = &t.colors;
    let mut line = row().w_full().min_h(36.0).px(10.0).py(6.0).gap(8.0).items_center()
        .child(ui::github::check_icon(t, check.status, None, 14.0))
        .child(
            row().grow(1.0).min_w(0.0).gap(6.0).items_center()
                .child(text(check.name.clone()).font(sans(12.0, 400.0, 16.0)).color(c.foreground).single_line().truncate().help(check.name.clone()))
                .children(check.workflow.as_ref().map(|w| text(w.clone()).font(sans(11.0, 400.0, 14.0)).color(c.muted_foreground).single_line().truncate())),
        );
    let open = check.job_id.is_some_and(|id| ui_state.logs.contains(&id));
    if let (CheckStatus::Failure, Some(job)) = (check.status, check.job_id) {
        let at = at.clone();
        line = line.child(
            row().key(format!("github-log-{job}")).role("button").label(format!("Log of {}", check.name)).checked(open).h(24.0).px(6.0).gap(4.0).items_center().radius(6.0).hover(c.foreground.alpha(0.03))
                .child(icon(glyph("ChevronRight"), 12.0).color(c.muted_foreground)).child(text("Log").font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                .on_press(move |s: &mut State| {
                    let opened = s.github_item_ui(&at).logs.insert(job);
                    if opened {
                        s.github_step(model::github::Event::LoadCheckLog(at.cwd.clone(), at.repo, job));
                    } else {
                        s.github_item_ui(&at).logs.remove(&job);
                    }
                }),
        );
    }
    if let Some(url) = &check.details_url {
        let url = url.clone();
        line = line.child(
            row().role("link").label(format!("Open {} on GitHub", check.name)).help("Open on GitHub".to_owned()).w(28.0).h(28.0).center().radius(6.0).hover(c.foreground.alpha(0.03))
                .child(icon(glyph("ExternalLink"), 14.0).color(c.muted_foreground))
                .on_press(move |s: &mut State| s.request_link_open(&url)),
        );
    }
    let mut out = col().w_full().border_sides([0.0, 0.0, 0.5, 0.0], c.border).child(line);
    if let (true, Some(job)) = (open, check.job_id) {
        let log = s.github.cache.log(&at.cwd, at.repo, job);
        let body = match (log.and_then(|l| l.data.as_ref()), log.map(|l| l.status)) {
            (Some(data), _) => {
                let mut body = col().w_full().gap(4.0);
                if data.truncated {
                    body = body.child(small(t, "Last lines of the job log"));
                }
                let tail = if data.tail.is_empty() { "The log is empty.".to_owned() } else { data.tail.clone() };
                body.child(ui::base::scroll_area::scroll_area(t, &format!("github-log-scroll-{job}"), el().padding(8.0).child(text(tail).font(mono(11.0, 400.0, 18.0)).color(c.foreground))).w_full().max_h(288.0).radius(6.0).fill(c.muted))
            }
            (None, Some(Status::Error)) => {
                let tail = log.and_then(|l| l.error.clone()).map_or(".".into(), |e| format!(": {e}"));
                text(format!("Couldn't load the log{tail}")).font(sans(12.0, 400.0, 16.0)).color(c.destructive)
            }
            _ => ui::base::skeleton::skeleton(t).w_full().h(96.0),
        };
        out = out.child(el().w_full().px(10.0).pb(10.0).child(body));
    }
    out
}

/// `GithubReviewThreads`: file and line, badges, comments and a Reply box. Resolved threads
/// start folded; Troupe never resolves a thread.
fn threads(t: &'static Tokens, at: &ItemRef, threads: &[ReviewThread], ui_state: &crate::github::ItemUi, phone: bool) -> El {
    let c = &t.colors;
    let unresolved = threads.iter().filter(|t| !t.is_resolved).count();
    let mut out = col().key("github-threads").role("region").label("Review threads").w_full().gap(8.0).child(
        row().gap(6.0).items_center()
            .child(text("Review threads").font(sans(14.0, 500.0, 20.0)).color(c.foreground))
            .child(small(t, format!("{unresolved} unresolved of {}", threads.len()))),
    );
    let now = crate::git_history::now_ms();
    for thread in threads {
        let open = thread.is_resolved == ui_state.flipped.contains(&thread.id);
        let (flip_at, flip_id) = (at.clone(), thread.id.clone());
        let mut location = text(thread.path.clone()).font(mono(11.0, 400.0, 16.0)).color(c.foreground).single_line().truncate().min_w(0.0);
        if let Some(line) = thread.line {
            location = rune::rich(format!("{}:{line}", thread.path), vec![rune::Span::new(thread.path.len(), thread.path.len() + 1 + line.to_string().len()).color(c.muted_foreground)])
                .font(mono(11.0, 400.0, 16.0)).color(c.foreground).single_line().truncate().min_w(0.0);
        }
        let mut badges = row().gap(4.0);
        if thread.is_resolved {
            badges = badges.child(row().h(20.0).px(6.0).gap(4.0).items_center().radius(10.0).fill(c.success.alpha(0.2)).child(icon(glyph("CheckCircle2"), 12.0).color(c.success)).child(text("Resolved").font(sans(10.0, 500.0, 14.0)).color(c.success)));
        }
        if thread.is_outdated {
            badges = badges.child(row().h(20.0).px(6.0).items_center().radius(10.0).fill(c.muted).child(text("Outdated").font(sans(10.0, 500.0, 14.0)).color(c.muted_foreground)));
        }
        let summary = row().key(format!("github-thread-{}", thread.id)).role("button").label(thread.path.clone()).checked(open).w_full().px(12.0).py(8.0).gap(8.0).items_center().radius(8.0).hover(c.foreground.alpha(0.03))
            .child(location).child(el().grow(1.0)).child(badges)
            .on_press(move |s: &mut State| {
                let item = s.github_item_ui(&flip_at);
                if !item.flipped.remove(&flip_id) {
                    item.flipped.insert(flip_id.clone());
                }
            });
        let mut card = col().w_full().min_w(0.0).radius(8.0).border(1.0, c.border).child(summary);
        if open {
            let mut body = col().w_full().gap(8.0).px(8.0).pb(8.0);
            for comment in &thread.comments {
                body = body.child(
                    col().w_full().min_w(0.0).radius(8.0).border(1.0, c.border).fill(c.card)
                        .child(row().w_full().px(12.0).py(6.0).gap(6.0).border_sides([0.0, 0.0, 0.5, 0.0], c.border)
                            .child(text(comment.author.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                            .child(small(t, crate::git_history::relative_time(&comment.created_at, now))))
                        .child(el().w_full().px(12.0).py(8.0).child(markdown(t, &comment.id, &comment.body, "No content."))),
                );
            }
            body = body.child(match ui_state.replies.get(&thread.id) {
                Some(draft) => composer(t, at, Some(&thread.id), draft, false, "Reply to this thread", "Reply", "Write a reply"),
                None => {
                    let (at, id) = (at.clone(), thread.id.clone());
                    row().w_full().child(Button::new("Reply").variant(ButtonVariant::Ghost).size(if phone { ButtonSize::Touch } else { ButtonSize::Xs }).build(t).key(format!("github-reply-{}", thread.id))
                        .grow(1.0)
                        .on_press(move |s: &mut State| {
                            s.github_item_ui(&at).replies.insert(id.clone(), crate::github::Draft::default());
                        }))
                }
            });
            card = card.child(body);
        }
        out = out.child(card);
    }
    out
}

/// The phone's item actions sheet (`GithubItemMenu`'s `MobileSheetContent`): the same rows as
/// the desktop menu, the close and reopen choices in their own group.
pub fn item_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let Some(at) = s.github_visible_item().filter(|at| s.github_is_open_for(ITEM_MENU, at)) else { return };
    let Some(data) = s.github.cache.item(&at).and_then(|r| r.data.clone()) else { return };
    let busy = s.github.items.get(&model::github::item_key(&at.cwd, at.repo, at.number)).is_some_and(|i| i.busy);
    let row_of = |name: &str, label: &str| ui::mobile::sheet_row(t, glyph(name), label, false);
    let labels: Vec<String> = data.item.labels.iter().map(|l| l.name.clone()).collect();
    let assignees = data.item.assignees.clone();
    let (labels_at, assignees_at) = (at.clone(), at.clone());
    let mut rows = vec![
        ui::mobile::sheet_subtitle(t, &data.repo.full_name()),
        row_of("Tag", "Edit labels").key("github-sheet-labels").on_press(move |s: &mut State| s.github_open_picker(PickerFor::Labels(labels_at.clone()), labels.clone())),
    ];
    if let Some((viewer, assigned, loading)) = self_action(s, &at, &data) {
        let at = at.clone();
        let label = if assigned { "Unassign me" } else { "Assign to me" };
        let r = row_of("User", label).key("github-sheet-self").opacity(if busy || loading { 0.5 } else { 1.0 });
        rows.push(if busy || loading { r } else { r.on_press(move |s: &mut State| s.github_toggle_self(&at, &viewer, assigned)) });
    }
    let (url, copy) = (data.item.url.clone(), data.item.url.clone());
    rows.push(row_of("User", "Edit assignees").key("github-sheet-assignees").on_press(move |s: &mut State| s.github_open_picker(PickerFor::Assignees(assignees_at.clone()), assignees.clone())));
    rows.push(row_of("ExternalLink", "Open on GitHub").on_press(move |s: &mut State| {
        s.close(ITEM_MENU);
        s.request_link_open(&url);
    }));
    rows.push(row_of("Copy", "Copy link").on_press_tx(move |s: &mut State, tx: &mut Tx| {
        s.close(ITEM_MENU);
        tx.copy(copy.clone());
        s.toasts.push(s.now, ui::overlay::ToastKind::Success, "Link copied", None);
    }));
    let mut sheet = crate::phone::sheet(ITEM_MENU, format!("{} #{}", ui::github::kind_noun(data.item.kind), data.item.number), s)
        .child(col().w_full().items_stretch().children(rows));
    let choices = logic::state_actions(data.item.kind, data.item.state);
    if !choices.is_empty() {
        let mut group = col().w_full().items_stretch();
        for choice in choices {
            let at = at.clone();
            let r = row_of(choice.icon, choice.label).opacity(if busy { 0.5 } else { 1.0 });
            group = group.child(if busy { r } else { r.on_press(move |s: &mut State| s.github_set_state(&at, choice.action, choice.label)) });
        }
        sheet = sheet.child(group);
    }
    sheet.build(t, window, layer);
}
