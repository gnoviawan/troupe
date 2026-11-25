//! The Changes side panel (`components/shell/ChangesPanel.tsx`): the header with the branch
//! button and the stash menu, the commit box, the filter, the staged and unstaged lists; and
//! the git dialogs (`GitPanelDialogs.tsx`): discard, amend, create branch, the branch switch
//! with uncommitted changes, and stash.

use model::git::StatusDetail;
use rune::{El, Tx, col, icon, row, text};
use theme::Tokens;
use ui::git::{self as parts, RowState, glyph};
use ui::overlay::{ConfirmDialog, ConfirmVariant, Dialog, Menu, MenuItem};
use ui::type_::{MICRO, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::git::*;

/// The panel's body under the shared header, for the active project's working copy.
pub fn changes_panel(s: &State, t: &'static Tokens, touch: bool) -> El {
    let Some(cwd) = s.git_cwd() else {
        return parts::empty_changes(t, Some("not a git repository"));
    };
    let body = col()
        .w_full()
        .child(commit_box(s, t, &cwd, touch))
        .child(col().w_full().px(8.0).pt(8.0).pb(2.0).child(
            parts::filter_field(t, &s.git.filter, touch)
                .key("git-filter")
                .on_edit(|s: &mut State| &mut s.git.filter),
        ))
        .child(change_list(s, t, &cwd, touch));
    col()
        .key("side-panel-changes")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(ui::base::scroll_area::scroll_area(t, "git-changes-scroll", body).grow(1.0).min_h(0.0))
        .child(crate::git_history_view::history_section(s, t, &cwd))
}

/// The header's actions: the branch button and the stash menu.
pub fn header_actions(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let Some(cwd) = s.git_cwd() else {
        return Vec::new();
    };
    vec![branch_button(s, t, &cwd, layer), stash_menu(s, t, &cwd, layer)]
}

/// `filteredStatuses` split into staged and unstaged rows.
fn lists<'a>(s: &'a State, cwd: &str) -> (Vec<&'a StatusDetail>, Vec<&'a StatusDetail>) {
    let query = s.git.filter.text.to_lowercase();
    s.git
        .cache
        .statuses(cwd)
        .iter()
        .filter(|f| query.is_empty() || f.path.to_lowercase().contains(&query))
        .partition(|f| f.staged)
}

fn change_list(s: &State, t: &'static Tokens, cwd: &str, touch: bool) -> El {
    let (staged, unstaged) = lists(s, cwd);
    let busy = s.git.busy();
    let selected = s.git.cache.selected_file.as_deref();
    let size = if touch { 16.0 } else { 13.0 };
    let mut list = col().key("git-change-list").w_full().px(8.0).pb(8.0).gap(8.0);
    if !staged.is_empty() {
        let all: Vec<String> = staged.iter().map(|f| f.path.clone()).collect();
        let unstage_all = parts::action(t, "Minus", "Unstage all changes", 13.0, busy, false)
            .key("git-unstage-all")
            .on_press(move |s: &mut State| s.git_unstage(all.clone()));
        let mut section = col().w_full().child(parts::section_header(t, "Staged", staged.len(), 0, vec![unstage_all]));
        for file in &staged {
            let path = file.path.clone();
            let unstage = parts::action(t, "Minus", "Unstage changes", size, busy, false)
                .key(format!("git-unstage-{path}"))
                .on_press({
                    let path = path.clone();
                    move |s: &mut State| s.git_unstage(vec![path.clone()])
                });
            let state = if selected == Some(path.as_str()) { RowState::Active } else { RowState::Idle };
            section = section.child(
                parts::file_row(t, &file.path, &file.status, state, touch, vec![unstage])
                    .key(format!("git-staged-{path}"))
                    .on_press(move |s: &mut State| s.git_open_diff(path.clone(), true)),
            );
        }
        list = list.child(section);
    }
    let mut actions = Vec::new();
    if !unstaged.is_empty() {
        let all: Vec<String> = unstaged.iter().map(|f| f.path.clone()).collect();
        let discard = all.clone();
        actions.push(
            parts::action(t, "RotateCcw", "Discard all changes", 13.0, busy, true)
                .key("git-discard-all")
                .on_press(move |s: &mut State| s.git_request_discard(discard.clone())),
        );
        actions.push(
            parts::action(t, "Plus", "Stage all changes", 13.0, busy, false)
                .key("git-stage-all")
                .on_press(move |s: &mut State| s.git_stage(all.clone())),
        );
    }
    let mut section = col().w_full().child(parts::section_header(t, "Changes", unstaged.len(), 0, actions));
    if unstaged.is_empty() {
        let error = s.git.cache.errors(cwd).status;
        section = section.child(parts::empty_changes(t, error.as_deref()));
    }
    for file in &unstaged {
        let path = file.path.clone();
        let discard = parts::action(t, "RotateCcw", "Discard changes", size, busy, true)
            .key(format!("git-discard-{path}"))
            .on_press({
                let path = path.clone();
                move |s: &mut State| s.git_request_discard(vec![path.clone()])
            });
        let stage = parts::action(t, "Plus", "Stage changes", size, busy, false)
            .key(format!("git-stage-{path}"))
            .on_press({
                let path = path.clone();
                move |s: &mut State| s.git_stage(vec![path.clone()])
            });
        let state = if selected == Some(path.as_str()) { RowState::Active } else { RowState::Idle };
        section = section.child(
            parts::file_row(t, &file.path, &file.status, state, touch, vec![discard, stage])
                .key(format!("git-unstaged-{path}"))
                .on_press(move |s: &mut State| s.git_open_diff(path.clone(), false)),
        );
    }
    list.child(section)
}

/// `CommitComposer`'s panel look.
fn commit_box(s: &State, t: &'static Tokens, cwd: &str, touch: bool) -> El {
    let c = &t.colors;
    let g = &s.git;
    let context = g.cache.commit_context(cwd);
    let has_head = context.is_some_and(|c| c.has_head);
    let field_h = if touch { 44.0 } else { 32.0 };
    let summary = ui::base::panel_styles::panel_field(t, &g.summary, if g.amend { "Update commit message" } else { "Summary (required)" }, touch)
        .key("git-summary")
        .w_full()
        .h(field_h)
        .py((field_h - 16.0) / 2.0)
        .on_edit(|s: &mut State| &mut s.git.summary)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.git_commit(false));
    let font = if touch { sans(16.0, 400.0, 24.0) } else { sans(MICRO, 400.0, 16.0) };
    let description = rune::text_input(&g.description, font)
        .label("Commit description".to_owned())
        .placeholder("Description (optional)".to_owned(), c.muted_foreground)
        .color(c.foreground)
        .key("git-description")
        .w_full()
        .h(56.0)
        .px(10.0)
        .py(6.0)
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border.alpha(0.6))
        .focus_ring(2.0, -1.0, c.focus_ring)
        .on_edit(|s: &mut State| &mut s.git.description);
    let amend_ink = if has_head { c.secondary_foreground } else { c.muted_foreground };
    let amend = row()
        .key("git-amend")
        .role("checkbox")
        .label("Amend last commit")
        .checked(g.amend)
        .help(if has_head { "Amend the last commit instead of creating a new one" } else { "No commit to amend yet" })
        .gap(8.0)
        .items_center()
        .min_w(0.0)
        .child(parts::checkbox(t, g.amend, !has_head))
        .child(text("Amend last commit").font(sans(MICRO, 400.0, 16.0)).color(amend_ink).single_line());
    let amend = if has_head && !g.committing { amend.on_press(|s: &mut State| s.git_toggle_amend()) } else { amend };
    let generate = row()
        .key("git-generate")
        .role("button")
        .label("Generate message")
        .help("Generating a message from staged changes lands with the agent picker")
        .h(26.0)
        .px(8.0)
        .gap(6.0)
        .items_center()
        .opacity(0.4)
        .child(icon(glyph("Sparkles"), 12.0).color(c.secondary_foreground))
        .child(text("Generate message").font(sans(MICRO, 500.0, 16.0)).color(c.secondary_foreground));
    let label = if g.committing {
        "Committing...".to_owned()
    } else if g.amend {
        "Amend commit".to_owned()
    } else {
        context.and_then(|c| c.branch.clone()).map_or_else(|| "Commit".to_owned(), |b| format!("Commit to {b}"))
    };
    let commit = Button::new(label)
        .size(ButtonSize::Xs)
        .icon(glyph("GitCommit"))
        .disabled(!s.git_can_commit())
        .build(t)
        .key("git-commit")
        .h(30.0)
        .grow(1.0)
        .min_w(0.0)
        .on_press(|s: &mut State| s.git_commit(false));
    let on_branch = context.is_some_and(|c| c.branch.is_some());
    let has_upstream = context.is_some_and(|c| c.has_upstream);
    let ahead = context.map_or(0, |c| c.ahead);
    let behind = context.map_or(0, |c| c.behind);
    let can_push = on_branch && (!has_upstream || ahead > 0) && !g.pushing && !g.committing;
    let push_label = if g.pushing {
        "Pushing...".to_owned()
    } else if !has_upstream {
        "Publish branch".to_owned()
    } else if ahead > 0 {
        format!("Push {ahead}")
    } else {
        "Up to date".to_owned()
    };
    let mut push = Button::new(push_label)
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Xs)
        .icon(glyph("ArrowUp"))
        .disabled(!can_push)
        .build(t)
        .key("git-push")
        .h(30.0)
        .on_press(|s: &mut State| s.git_push());
    if behind > 0 {
        push = push.child(text(format!("\u{2193}{behind}")).font(sans(MICRO, 400.0, 16.0)).color(c.warning));
    }
    col()
        .key("git-commit-box")
        .w_full()
        .gap(8.0)
        .padding(12.0)
        .border_sides([0.0, 0.0, 0.5, 0.0], c.border)
        .child(summary)
        .child(description)
        .child(row().w_full().justify_between().items_center().gap(8.0).child(amend).child(generate))
        .child(row().w_full().gap(8.0).child(commit).child(push))
}

/// `BranchSection`'s panel trigger and its menu: create, then every local branch.
fn branch_button(s: &State, t: &'static Tokens, cwd: &str, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let context = s.git.cache.commit_context(cwd);
    let current = context.and_then(|c| c.branch.clone());
    let ahead = context.map_or(0, |c| c.ahead);
    let mut trigger = row()
        .key("git-branch-button")
        .role("button")
        .label("Switch branch")
        .h(26.0)
        .max_w(160.0)
        .min_w(0.0)
        .px(8.0)
        .gap(6.0)
        .items_center()
        .radius(6.0)
        .fill(c.secondary)
        .hover(c.secondary.alpha(0.8))
        .child(icon(glyph("GitBranch"), 13.0).color(c.muted_foreground))
        .child(
            text(current.clone().unwrap_or_else(|| "Detached HEAD".to_owned()))
                .font(sans(MICRO, 500.0, 16.0))
                .color(c.foreground)
                .single_line()
                .truncate()
                .min_w(0.0),
        );
    if ahead > 0 {
        trigger = trigger.child(text(format!("\u{2191}{ahead}")).font(sans(MICRO, 400.0, 16.0)).color(c.muted_foreground));
    }
    trigger = trigger
        .child(icon(glyph("ChevronDown"), 11.0).color(c.muted_foreground))
        .on_press(|s: &mut State| s.overlays.open(BRANCH_MENU));
    let mut menu = Menu::new(BRANCH_MENU)
        .width(224.0)
        .open(s.overlays.is_open(BRANCH_MENU))
        .on_close(|s: &mut State| s.close(BRANCH_MENU))
        .item(MenuItem::new("Create new branch...").icon(glyph("Plus")).on_select(|s: &mut State| {
            s.close(BRANCH_MENU);
            s.overlays.open(CREATE_BRANCH);
        }))
        .separator();
    let branches = s.git.cache.branches(cwd);
    if branches.is_empty() {
        menu = menu.item(MenuItem::new("No branches found").disabled(true));
    }
    for branch in branches {
        let name = branch.clone();
        let here = current.as_deref() == Some(branch.as_str());
        menu = menu.item(MenuItem::new(branch.clone()).checked(here).on_select(move |s: &mut State| {
            if here {
                s.close(BRANCH_MENU);
            } else {
                s.git_switch_branch(name.clone());
            }
        }));
    }
    menu.build(t, layer, trigger)
}

/// `StashMenu`: stash the changes, then each stash with apply, pop and drop.
fn stash_menu(s: &State, t: &'static Tokens, cwd: &str, layer: &mut Vec<El>) -> El {
    let stashes = s.git.cache.stashes(cwd);
    let label = if stashes.is_empty() { "Stash changes".to_owned() } else { format!("Stashes ({})", stashes.len()) };
    let trigger = ui::base::panel_styles::panel_icon_button(t, glyph("Archive"), label)
        .key("git-stash-button")
        .on_press(|s: &mut State| s.overlays.open(STASH_MENU));
    let can_stash = !s.git.cache.statuses(cwd).is_empty() && !s.git.busy();
    let mut menu = Menu::new(STASH_MENU)
        .width(256.0)
        .open(s.overlays.is_open(STASH_MENU))
        .on_close(|s: &mut State| s.close(STASH_MENU))
        .item(MenuItem::new("Stash changes").icon(glyph("Archive")).disabled(!can_stash).on_select(|s: &mut State| {
            s.close(STASH_MENU);
            s.overlays.open(STASH_DIALOG);
        }));
    if !stashes.is_empty() {
        menu = menu.separator().label("Stashes");
    }
    for stash in stashes {
        let index = stash.index;
        let ops = Menu::new(format!("{STASH_MENU}-{index}"))
            .width(176.0)
            .item(MenuItem::new("Apply").icon(glyph("ClipboardPaste")).on_select(move |s: &mut State| s.git_stash(StashOp::Apply, index)))
            .item(MenuItem::new("Pop").icon(glyph("ArchiveRestore")).on_select(move |s: &mut State| s.git_stash(StashOp::Pop, index)))
            .separator()
            .item(MenuItem::new("Drop").icon(glyph("Trash2")).destructive().on_select(move |s: &mut State| s.git_stash(StashOp::Drop, index)));
        menu = menu.item(MenuItem::new(format!("stash@{{{index}}}  {}", stash.message)).submenu(ops));
    }
    menu.build(t, layer, trigger)
}

/// The dialogs, mounted in the overlay layer.
pub fn dialogs(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let g = &s.git;
    if s.overlays.is_open(DISCARD_CONFIRM) {
        let count = g.discard.len();
        let message = if count == 1 {
            format!("Discard changes to \"{}\"? This cannot be undone.", g.discard[0])
        } else {
            format!("Discard changes to {count} files? This cannot be undone.")
        };
        ConfirmDialog::new(DISCARD_CONFIRM, "Discard changes", message)
            .variant(ConfirmVariant::Danger)
            .confirm_label("Discard")
            .loading(g.mutating)
            .open(true)
            .on_cancel(|s: &mut State| {
                s.git.discard.clear();
                s.close(DISCARD_CONFIRM);
            })
            .on_confirm(|s: &mut State| s.git_confirm_discard())
            .build(t, layer);
    }
    if s.overlays.is_open(AMEND_CONFIRM) {
        ConfirmDialog::new(
            AMEND_CONFIRM,
            "Amend pushed commit",
            "The last commit appears to already be pushed. Amending rewrites published history and will require a force-push to update the remote. Continue?",
        )
        .variant(ConfirmVariant::Danger)
        .confirm_label("Amend anyway")
        .loading(g.committing)
        .open(true)
        .on_cancel(|s: &mut State| s.close(AMEND_CONFIRM))
        .on_confirm(|s: &mut State| s.git_commit(true))
        .build(t, layer);
    }
    if s.overlays.is_open(CREATE_BRANCH) {
        create_branch(s, t, layer);
    }
    if s.overlays.is_open(SWITCH_BRANCH) {
        switch_branch(s, t, layer);
    }
    if s.overlays.is_open(STASH_DIALOG) {
        stash_dialog(s, t, layer);
    }
}

fn caption(t: &Tokens, label: &str) -> El {
    text(label.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground)
}

fn create_branch(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let name = ui::base::panel_styles::panel_field(t, &s.git.new_branch, "e.g. feature/new-login", false)
        .key("git-new-branch")
        .w_full()
        .h(32.0)
        .autofocus()
        .on_edit(|s: &mut State| &mut s.git.new_branch)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.git_create_branch());
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("git-branch-cancel").on_press(|s: &mut State| s.close(CREATE_BRANCH));
    let create = Button::new("Create & Switch")
        .size(ButtonSize::Sm)
        .disabled(s.git.new_branch.text.trim().is_empty())
        .build(t)
        .key("git-branch-create")
        .on_press(|s: &mut State| s.git_create_branch());
    Dialog::new(CREATE_BRANCH, "Create New Branch")
        .width(425.0)
        .child(col().w_full().gap(4.0).child(caption(t, "Branch name")).child(name))
        .footer(cancel)
        .footer(create)
        .open(true)
        .on_close(|s: &mut State| s.close(CREATE_BRANCH))
        .build(t, layer);
}

fn switch_branch(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let name = s.git.pending_branch.clone().unwrap_or_default();
    let note = |line: &str| text(line.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap().w_full();
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("git-switch-cancel").on_press(|s: &mut State| {
        s.git.pending_branch = None;
        s.close(SWITCH_BRANCH);
    });
    let stash = Button::new("Stash & Switch")
        .size(ButtonSize::Sm)
        .build(t)
        .key("git-switch-stash")
        .on_press(|s: &mut State| s.git_execute_switch(true));
    let bring = Button::new("Bring Changes").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key("git-switch-bring").on_press(|s: &mut State| s.git_execute_switch(false));
    Dialog::new(SWITCH_BRANCH, "Uncommitted Changes")
        .width(425.0)
        .child(note(&format!(
            "You have uncommitted changes on your current branch. How would you like to handle them before switching to {name}?"
        )))
        .child(note("\u{2022} Bring Changes: Keep your changes and carry them over to the new branch."))
        .child(note("\u{2022} Stash & Switch: Stash your changes on this branch, switch, and try to re-apply them on the new branch."))
        .footer(cancel)
        .footer(bring)
        .footer(stash)
        .open(true)
        .on_close(|s: &mut State| {
            s.git.pending_branch = None;
            s.close(SWITCH_BRANCH);
        })
        .build(t, layer);
}

fn stash_dialog(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let message = ui::base::panel_styles::panel_field(t, &s.git.stash_message, "WIP on current branch...", false)
        .key("git-stash-message")
        .w_full()
        .h(32.0)
        .autofocus()
        .on_edit(|s: &mut State| &mut s.git.stash_message)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.git_stash_save());
    let untracked = row()
        .key("git-stash-untracked")
        .role("checkbox")
        .label("Include untracked files")
        .checked(s.git.stash_untracked)
        .gap(8.0)
        .items_center()
        .child(parts::checkbox(t, s.git.stash_untracked, false))
        .child(text("Include untracked files").font(sans(12.0, 400.0, 16.0)).color(t.colors.foreground))
        .on_press(|s: &mut State| s.git.stash_untracked = !s.git.stash_untracked);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("git-stash-cancel").on_press(|s: &mut State| s.close(STASH_DIALOG));
    let save = Button::new("Stash").size(ButtonSize::Sm).build(t).key("git-stash-save").on_press(|s: &mut State| s.git_stash_save());
    Dialog::new(STASH_DIALOG, "Stash Changes")
        .width(425.0)
        .child(col().w_full().gap(4.0).child(caption(t, "Message (optional)")).child(message))
        .child(untracked)
        .footer(cancel)
        .footer(save)
        .open(true)
        .on_close(|s: &mut State| s.close(STASH_DIALOG))
        .build(t, layer);
}

/// The phone sheet's body (`GitPanel`'s mobile variant): the file list with touch rows, or,
/// with a file selected, that file's diff and a back button.
pub fn phone_panel(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) -> El {
    let height = (window.1 * 0.9 - 72.0).max(320.0);
    let Some(cwd) = s.git_cwd() else {
        return parts::empty_changes(t, Some("not a git repository"));
    };
    if let Some(file) = s.git.cache.selected_file.clone() {
        let c = &t.colors;
        let back = row()
            .key("git-sheet-back")
            .role("button")
            .label("Back to changes")
            .min_h(44.0)
            .px(8.0)
            .gap(8.0)
            .items_center()
            .child(icon(glyph("ChevronLeft"), 16.0).color(c.muted_foreground))
            .child(text(file.clone()).font(sans(14.0, 500.0, 20.0)).color(c.foreground).single_line().truncate().min_w(0.0))
            .on_press(|s: &mut State| {
                s.git_step(model::git::Event::SetSelectedFile(None));
            });
        let body = crate::git_diff::diff_body(s, t, GIT_SHEET, &cwd, &file, s.git.selected_staged);
        return col().key("git-sheet-diff").w_full().h(height).child(back).child(body);
    }
    let mut actions = header_actions(s, t, layer);
    let branch = actions.remove(0);
    let header = row()
        .key("git-sheet-header")
        .w_full()
        .min_h(56.0)
        .pl(4.0)
        .gap(4.0)
        .items_center()
        .child(branch)
        .child(row().grow(1.0))
        .children(actions);
    let body = col()
        .w_full()
        .child(col().w_full().px(8.0).pb(8.0).child(
            parts::filter_field(t, &s.git.filter, true).key("git-filter").on_edit(|s: &mut State| &mut s.git.filter),
        ))
        .child(change_list(s, t, &cwd, true));
    col()
        .key("git-sheet-body")
        .w_full()
        .h(height)
        .child(header)
        .child(ui::base::scroll_area::scroll_area(t, "git-sheet-scroll", body).grow(1.0).min_h(0.0))
        .child(commit_box(s, t, &cwd, true))
}
