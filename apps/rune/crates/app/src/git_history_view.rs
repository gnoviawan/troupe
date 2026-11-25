//! The history surfaces: the Changes panel's History section (`HistorySection`), the Git
//! History tab (`GitHistoryView` with its toolbar, scope and author menus, commit table and
//! detail pane), and the commit diff tab (`CommitDiffTab`).

use model::workspace::Tab;
use rune::{El, col, el, icon, row, text};
use rune_kit::diff::{DiffState, DiffView};
use theme::Tokens;
use ui::git::glyph;
use ui::overlay::{Menu, MenuItem};
use ui::type_::{MICRO, mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::git_history::*;

/// The section at the bottom of the Changes panel.
pub fn history_section(s: &State, t: &'static Tokens, cwd: &str) -> El {
    let c = &t.colors;
    let summary = s.history.head.get(cwd);
    let open = !s.history.section_closed;
    let label = summary.map_or(String::new(), |(_, count)| count_label(*count));
    let toggle = row()
        .key("git-history-toggle")
        .role("button")
        .label("History")
        .checked(open)
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .gap(6.0)
        .items_center()
        .child(icon(glyph(if open { "ChevronDown" } else { "ChevronRight" }), 12.0).color(c.muted_foreground))
        .child(text("History").font(sans(MICRO, 500.0, 16.0)).color(c.secondary_foreground))
        .child(text(label).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0))
        .on_press(|s: &mut State| s.history.section_closed = !s.history.section_closed);
    let view_cwd = cwd.to_owned();
    let view_all = row()
        .key("git-history-view-all")
        .role("button")
        .label("Open History in the main panel")
        .h(24.0)
        .px(7.0)
        .gap(5.0)
        .items_center()
        .radius(6.0)
        .hover(c.foreground.alpha(0.03))
        .child(text("View all").font(sans(MICRO, 500.0, 16.0)).color(c.secondary_foreground))
        .child(icon(glyph("ExternalLink"), 12.0).color(c.secondary_foreground))
        .on_press(move |s: &mut State| {
            s.workspace.step(model::workspace::Event::AddGitHistoryTab(view_cwd.clone(), None));
        });
    let mut section = col()
        .key("git-history-section")
        .role("region")
        .label("History")
        .w_full()
        .pb(6.0)
        .border_sides([0.5, 0.0, 0.0, 0.0], c.border)
        .child(row().w_full().h(36.0).pl(12.0).pr(6.0).gap(6.0).items_center().child(toggle).child(view_all));
    if !open {
        return section;
    }
    let mut list = col().w_full().px(6.0);
    match summary {
        None => list = list.child(quiet(t, "Loading...")),
        Some((recent, _)) if recent.is_empty() => list = list.child(quiet(t, "No commits yet")),
        Some((recent, count)) => {
            let now = now_ms();
            for (index, commit) in recent.iter().enumerate() {
                let below = index + 1 < recent.len() || *count > recent.len();
                list = list.child(recent_row(t, commit, index == 0, index > 0, below, now));
            }
            if *count > recent.len() {
                let cwd = cwd.to_owned();
                list = list.child(
                    row()
                        .key("git-history-show-all")
                        .role("button")
                        .h(28.0)
                        .px(6.0)
                        .gap(8.0)
                        .items_center()
                        .radius(6.0)
                        .hover(c.foreground.alpha(0.03))
                        .child(el().w(12.0))
                        .child(text(format!("Show all {}", count_label(*count))).font(sans(MICRO, 500.0, 16.0)).color(c.muted_foreground))
                        .on_press(move |s: &mut State| {
                            s.workspace.step(model::workspace::Event::AddGitHistoryTab(cwd.clone(), None));
                        }),
                );
            }
        }
    }
    section = section.child(list);
    section
}

fn quiet(t: &Tokens, line: &str) -> El {
    row().h(26.0).px(6.0).items_center().child(text(line.to_owned()).font(sans(MICRO, 400.0, 16.0)).color(t.colors.muted_foreground))
}

/// The graph gutter of one row: the line above and below and the commit's dot.
fn dot(t: &Tokens, head: bool, above: bool, below: bool, height: f32) -> El {
    let c = &t.colors;
    let half = height / 2.0;
    let mut g = col().w(12.0).h(height).items_center();
    g = g.child(el().w(1.5).h(half - 3.5).fill(if above { c.primary } else { rune::Color::TRANSPARENT }));
    g = g.child(if head {
        el().w(7.5).h(7.5).radius(4.0).fill(c.card).border(1.5, c.primary)
    } else {
        el().w(7.0).h(7.0).radius(3.5).fill(c.primary)
    });
    g.child(el().w(1.5).grow(1.0).fill(if below { c.primary } else { rune::Color::TRANSPARENT })).a11y_hidden()
}

fn ref_chip(t: &Tokens, raw: &str) -> El {
    let c = &t.colors;
    let (label, head) = describe_ref(raw);
    let chip = row().h(16.0).max_w(96.0).px(5.0).items_center().radius(8.0);
    let chip = if head { chip.fill(c.primary.alpha(0.16)) } else { chip.border(0.5, c.border) };
    chip.child(text(label).font(sans(10.0, 500.0, 12.0)).color(if head { c.primary } else { c.muted_foreground }).single_line().truncate())
}

fn recent_row(t: &Tokens, commit: &Commit, head: bool, above: bool, below: bool, now: f64) -> El {
    let c = &t.colors;
    let subject = if commit.subject.is_empty() { "(no message)".to_owned() } else { commit.subject.clone() };
    let mut r = row()
        .key(format!("git-recent-{}", commit.hash))
        .h(26.0)
        .px(6.0)
        .gap(8.0)
        .items_center()
        .help(subject.clone())
        .child(dot(t, head, above, below, 26.0))
        .child(
            text(subject)
                .font(sans(MICRO, if head { 500.0 } else { 400.0 }, 16.0))
                .color(if head { c.foreground } else { c.secondary_foreground })
                .single_line()
                .truncate()
                .min_w(0.0)
                .grow(1.0),
        );
    if commit.refs.is_empty() {
        r = r.child(text(relative_time(&commit.date, now)).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground));
    } else {
        r = r.child(row().gap(4.0).children(commit.refs.iter().take(2).map(|raw| ref_chip(t, raw))));
    }
    r
}

/// The Git History tab's body, or `None` for another kind.
pub fn tab_body(s: &State, t: &'static Tokens, tab: &Tab, layer: &mut Vec<El>) -> Option<El> {
    match tab {
        Tab::GitHistory { id, cwd } => Some(history_tab(s, t, id, cwd, layer)),
        Tab::CommitDiff { id, path, commit, .. } => Some(commit_diff(s, t, id, path, commit)),
        _ => None,
    }
}

fn history_tab(s: &State, t: &'static Tokens, id: &str, cwd: &str, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let all = match s.history.log.get(cwd) {
        Some(Read::Ready(commits)) => commits.len(),
        _ => 0,
    };
    let visible = s.visible_commits(cwd);
    let filtering = !s.history.query.text.trim().is_empty() || s.history.author.is_some();
    let count = if filtering { format!("{} of {all}", visible.len()) } else { format!("{all} {}", if all == 1 { "commit" } else { "commits" }) };
    let toolbar = toolbar(s, t, cwd, count, layer);
    let body = match s.history.log.get(cwd) {
        None | Some(Read::Loading) => center_note(t, "Loading history..."),
        Some(Read::Failed(_)) | Some(Read::Ready(_)) if all == 0 => center_note(t, "No commit history"),
        _ => {
            let header = row()
                .w_full()
                .h(28.0)
                .pr(16.0)
                .items_center()
                .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
                .child(head_cell(t, if filtering { "" } else { "Graph" }).w(40.0).pl(12.0))
                .child(head_cell(t, "Message").grow(1.0).min_w(0.0))
                .child(head_cell(t, "Author").w(160.0).pl(12.0))
                .child(head_cell(t, "Date").w(80.0))
                .child(head_cell(t, "Hash").w(80.0).pl(16.0));
            let selected = s.history.selected.get(cwd).cloned();
            let now = now_ms();
            let mut rows = col().key(format!("git-history-rows-{id}")).w_full().role("listbox").label("Commits");
            if filtering && visible.is_empty() {
                rows = rows.child(center_note(t, "No commits match this filter.").py(32.0));
            }
            let last = visible.len().saturating_sub(1);
            for (index, commit) in visible.iter().enumerate() {
                rows = rows.child(commit_row(t, cwd, commit, selected.as_deref() == Some(commit.hash.as_str()), index == 0, index > 0, index < last, !filtering, now));
            }
            let list = col()
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .child(header)
                .child(ui::base::scroll_area::scroll_area(t, &format!("git-history-scroll-{id}"), rows).grow(1.0).min_h(0.0));
            let mut split = row().w_full().grow(1.0).min_h(0.0).child(list);
            if let Some(hash) = selected.filter(|_| !s.history.detail_hidden)
                && let Some(commit) = visible.iter().find(|c| c.hash == hash)
            {
                // React's narrow layout: the detail pane floats over the table's right edge,
                // so the table keeps its columns.
                split = split.child(detail_pane(s, t, cwd, commit).fill(c.background).pin(None, Some(0.0), Some(0.0), Some(0.0)));
            }
            split
        }
    };
    col().key(format!("tab-body-{id}")).w_full().grow(1.0).min_h(0.0).child(toolbar).child(body)
}

fn head_cell(t: &Tokens, label: &str) -> El {
    row().child(text(label.to_owned()).font(sans(MICRO, 500.0, 16.0)).color(t.colors.muted_foreground).single_line())
}

fn center_note(t: &Tokens, line: &str) -> El {
    col().w_full().grow(1.0).center().child(text(line.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground))
}

#[allow(clippy::too_many_arguments)]
fn commit_row(t: &Tokens, cwd: &str, commit: &Commit, selected: bool, head: bool, above: bool, below: bool, graph: bool, now: f64) -> El {
    let c = &t.colors;
    let (cwd_owned, hash) = (cwd.to_owned(), commit.hash.clone());
    let subject = if commit.subject.is_empty() { "(no message)".to_owned() } else { commit.subject.clone() };
    let mut message = row().grow(1.0).min_w(0.0).gap(6.0).items_center();
    for raw in commit.refs.iter().take(3) {
        message = message.child(ref_chip(t, raw));
    }
    message = message.child(text(subject.clone()).font(sans(12.0, 400.0, 16.0)).color(c.foreground).single_line().truncate().min_w(0.0));
    let gutter = if graph { row().w(40.0).pl(14.0).child(dot(t, head, above, below, 28.0)) } else { row().w(40.0) };
    let r = row()
        .key(format!("git-commit-{}", commit.hash))
        .role("option")
        .label(subject)
        .checked(selected)
        .w_full()
        .h(28.0)
        .pr(16.0)
        .items_center()
        .child(gutter)
        .child(message)
        .child(text(commit.author.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().w(160.0).pl(12.0))
        .child(row().w(80.0).justify_end().child(text(relative_time(&commit.date, now)).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)))
        .child(text(commit.short_hash.clone()).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).w(80.0).pl(16.0))
        .on_press(move |s: &mut State| s.select_commit(&cwd_owned, &hash));
    if selected { r.fill(c.row_hover_solid) } else { r.hover(c.foreground.alpha(0.03)) }
}

/// `HistoryToolbar`: the scope menu, the filter, the author menu, the count and refresh.
fn toolbar(s: &State, t: &'static Tokens, cwd: &str, count: String, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let scope = s.history.scope.get(cwd).copied().unwrap_or_default();
    // The pill shows its value as plain text, so the page's text reads it; a transparent
    // button over it carries the trigger's name, as React's `aria-label` does.
    let trigger = |key: &str, label: String, aria: String| {
        row()
            .h(28.0)
            .px(8.0)
            .gap(6.0)
            .items_center()
            .radius(6.0)
            .border(1.0, c.border.alpha(0.6))
            .child(text(label).font(sans(12.0, 400.0, 16.0)).color(c.foreground).single_line())
            .child(icon(glyph("ChevronDown"), 11.0).color(c.muted_foreground))
            .child(
                el().key(key.to_owned())
                    .role("button")
                    .label(aria)
                    .radius(6.0)
                    .hover(c.foreground.alpha(0.03))
                    .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)),
            )
    };
    let scope_trigger = trigger("git-history-scope", scope.label().to_owned(), format!("Scope: {}", scope.label()))
        .on_press(|s: &mut State| s.overlays.open(SCOPE_MENU));
    let (all_cwd, head_cwd) = (cwd.to_owned(), cwd.to_owned());
    let scope_menu = Menu::new(SCOPE_MENU)
        .width(176.0)
        .open(s.overlays.is_open(SCOPE_MENU))
        .on_close(|s: &mut State| s.close(SCOPE_MENU))
        .item(MenuItem::new("All branches").checked(scope == Scope::All).on_select(move |s: &mut State| s.set_history_scope(&all_cwd, Scope::All)))
        .item(MenuItem::new("Current branch").checked(scope == Scope::Head).on_select(move |s: &mut State| s.set_history_scope(&head_cwd, Scope::Head)))
        .build(t, layer, scope_trigger);
    let filter = ui::base::panel_styles::panel_field(t, &s.history.query, "Filter by message, author or hash", false)
        .key("git-history-filter")
        .w(260.0)
        .h(28.0)
        .on_edit(|s: &mut State| &mut s.history.query);
    let mut authors: Vec<String> = match s.history.log.get(cwd) {
        Some(Read::Ready(commits)) => commits.iter().map(|c| c.author.clone()).collect(),
        _ => Vec::new(),
    };
    authors.sort();
    authors.dedup();
    let current = s.history.author.clone();
    let author_aria = current.as_ref().map_or_else(|| "Filter by author".to_owned(), |a| format!("Author: {a}"));
    let author_trigger = trigger("git-history-author", current.clone().unwrap_or_else(|| "Any author".to_owned()), author_aria)
        .on_press(|s: &mut State| s.overlays.open(AUTHOR_MENU));
    let mut author_menu = Menu::new(AUTHOR_MENU)
        .width(208.0)
        .open(s.overlays.is_open(AUTHOR_MENU))
        .on_close(|s: &mut State| s.close(AUTHOR_MENU))
        .item(MenuItem::new("Any author").checked(current.is_none()).on_select(|s: &mut State| {
            s.history.author = None;
            s.close(AUTHOR_MENU);
        }))
        .separator();
    for author in authors {
        let pick = author.clone();
        author_menu = author_menu.item(MenuItem::new(author.clone()).checked(current.as_deref() == Some(author.as_str())).on_select(move |s: &mut State| {
            s.history.author = Some(pick.clone());
            s.close(AUTHOR_MENU);
        }));
    }
    let author_menu = author_menu.build(t, layer, author_trigger);
    let refresh_cwd = cwd.to_owned();
    let refresh = ui::base::panel_styles::panel_icon_button(t, glyph("RefreshCw"), "Refresh history")
        .key("git-history-refresh")
        .on_press(move |s: &mut State| s.refresh_log(&refresh_cwd));
    row()
        .w_full()
        .h(44.0)
        .px(12.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(scope_menu)
        .child(filter)
        .child(author_menu)
        .child(row().grow(1.0))
        .child(text(count).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground))
        .child(refresh)
}

/// `CommitDetailPane`.
fn detail_pane(s: &State, t: &'static Tokens, cwd: &str, commit: &Commit) -> El {
    let c = &t.colors;
    let key = (cwd.to_owned(), commit.hash.clone());
    let entry = s.history.details.get(&key);
    let detail = match entry {
        Some(Read::Ready(d)) => Some(d),
        _ => None,
    };
    let body = detail.map(|d| d.message.split_once('\n').map_or("", |(_, rest)| rest).trim().to_owned()).unwrap_or_default();
    let meta = |label: &str, glyph_name: &str, value: String, mono_text: bool| {
        row()
            .gap(8.0)
            .items_center()
            .min_w(0.0)
            .help(label.to_owned())
            .child(icon(glyph(glyph_name), 14.0).color(c.muted_foreground))
            .child(text(value).font(if mono_text { mono(12.0, 400.0, 16.0) } else { sans(12.0, 400.0, 16.0) }).color(c.foreground.alpha(0.8)).single_line().truncate().min_w(0.0))
    };
    let author = match detail.map(|d| d.author_email.as_str()).filter(|e| !e.is_empty()) {
        Some(email) => format!("{} <{email}>", commit.author),
        None => commit.author.clone(),
    };
    let close = ui::base::panel_styles::panel_icon_button(t, glyph("X"), "Close details").on_press(|s: &mut State| s.history.detail_hidden = true);
    let first_file = detail.and_then(|d| d.files.first().cloned());
    let (open_cwd, open_hash) = (cwd.to_owned(), commit.hash.clone());
    let open = Button::new("Open diff")
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Sm)
        .icon(glyph("FileDiff"))
        .disabled(first_file.is_none())
        .build(t)
        .key("git-open-diff")
        .on_press(move |s: &mut State| {
            if let Some(file) = &first_file {
                s.open_commit_file(&open_cwd, &open_hash, file);
            }
        });
    let mut top = col()
        .w_full()
        .gap(10.0)
        .padding(16.0)
        .child(row().w_full().gap(6.0).children(commit.refs.iter().map(|r| ref_chip(t, r))).child(row().grow(1.0)).child(close))
        .child(text(if commit.subject.is_empty() { "(no message)".to_owned() } else { commit.subject.clone() }).font(sans(16.0, 500.0, 22.0)).color(c.foreground).wrap().w_full());
    if !body.is_empty() {
        top = top.child(text(body).font(sans(12.0, 400.0, 19.5)).color(c.muted_foreground).wrap().w_full());
    }
    top = top
        .child(meta("Author", "User", author, false))
        .child(meta("Date", "Clock", commit.date.clone(), false))
        .child(meta("Hash", "GitCommit", commit.hash.clone(), true));
    if !commit.parents.is_empty() {
        let parents = commit.parents.iter().map(|p| p[..p.len().min(7)].to_owned()).collect::<Vec<_>>().join(" ");
        top = top.child(meta(if commit.parents.len() > 1 { "Parents" } else { "Parent" }, "GitMerge", parents, true));
    }
    top = top.child(row().gap(8.0).child(open));
    let files_label = match detail {
        Some(d) => format!("{}{} {} changed", d.files.len(), if d.files_truncated { "+" } else { "" }, if d.files.len() == 1 { "file" } else { "files" }),
        None => "Files".to_owned(),
    };
    let mut files = col().w_full().px(8.0).pb(12.0);
    match entry {
        Some(Read::Failed(error)) => files = files.child(center_note(t, &format!("Couldn't load this commit: {error}")).py(24.0)),
        Some(Read::Ready(d)) => {
            for file in &d.files {
                let (cwd, hash, f) = (cwd.to_owned(), commit.hash.clone(), file.clone());
                let (name, dir) = ui::git::split_path(&file.path);
                let counts = match (file.added, file.removed) {
                    (Some(a), Some(r)) => format!("+{a} -{r}"),
                    _ => String::new(),
                };
                files = files.child(
                    row()
                        .key(format!("git-commit-file-{}", file.path))
                        .role("button")
                        .label(file.path.clone())
                        .h(28.0)
                        .px(8.0)
                        .gap(8.0)
                        .items_center()
                        .radius(6.0)
                        .hover(c.foreground.alpha(0.03))
                        .child(icon(glyph("File"), 14.0).color(c.project_blue))
                        .child(text(name.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.foreground).single_line())
                        .child(text(dir.to_owned()).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0).grow(1.0))
                        .child(text(counts).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground))
                        .child(ui::git::status_mark(t, &file.status))
                        .on_press(move |s: &mut State| s.open_commit_file(&cwd, &hash, &f)),
                );
            }
        }
        _ => files = files.child(center_note(t, "Loading...").py(24.0)),
    }
    let content = col()
        .w_full()
        .child(top)
        .child(row().w_full().h(36.0).px(16.0).items_center().border_sides([1.0, 0.0, 0.0, 0.0], c.border).child(text(files_label).font(sans(MICRO, 500.0, 16.0)).color(c.muted_foreground)))
        .child(files);
    col()
        .key("git-commit-detail")
        .role("complementary")
        .label("Commit detail")
        .w(400.0)
        .h_full()
        .border_sides([0.0, 0.0, 0.0, 1.0], c.border)
        .child(ui::base::scroll_area::scroll_area(t, "git-commit-detail-scroll", content).grow(1.0).min_h(0.0))
}

thread_local! {
    static EMPTY: &'static DiffState = Box::leak(Box::new(DiffState::new(Vec::new())));
}

/// `CommitDiffTab`: one file of one commit against its first parent.
fn commit_diff(s: &State, t: &'static Tokens, id: &str, path: &str, commit: &str) -> El {
    let c = &t.colors;
    let header = row()
        .w_full()
        .h(36.0)
        .px(12.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(icon(glyph("FileDiff"), 13.0).color(c.muted_foreground))
        .child(text(path.to_owned()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).single_line().truncate().min_w(0.0).grow(1.0))
        .child(text(commit[..commit.len().min(7)].to_owned()).font(mono(MICRO, 400.0, 16.0)).color(c.muted_foreground));
    let body = match s.history.diffs.get(id) {
        Some(Read::Ready((_, true))) => center_note(t, "This diff is too large to show."),
        Some(Read::Ready((patch, _))) if patch.trim().is_empty() => center_note(t, "No changes to show"),
        Some(Read::Ready(_)) => {
            let (get, get_mut) = (id.to_owned(), id.to_owned());
            DiffView::new(
                move |s: &State| s.history.diff_views.get(&get).unwrap_or_else(|| EMPTY.with(|e| *e)),
                move |s: &mut State| s.history.diff_views.entry(get_mut.clone()).or_insert_with(|| DiffState::new(Vec::new())),
            )
            .key(format!("diff-view-{id}"))
            .build(s, theme::kit(s.theme, s.dark))
            .grow(1.0)
            .min_h(0.0)
        }
        Some(Read::Failed(error)) => center_note(t, &format!("Couldn't load the diff: {error}")),
        _ => center_note(t, "Loading diff..."),
    };
    col().key(format!("tab-body-{id}")).w_full().grow(1.0).min_h(0.0).child(header).child(body)
}
