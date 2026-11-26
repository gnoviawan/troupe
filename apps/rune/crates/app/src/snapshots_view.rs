//! The snapshots page (`WorkspaceSnapshots`) and its dialogs (`CreateSnapshotModal`,
//! `RestoreSnapshotModal`, `DeleteSnapshotModal`). The page takes the main area; below the
//! phone breakpoint it is the phone variant: a 48 px header, stacked cards with their actions
//! always shown, and dialogs whose buttons stack full width with the confirm on top.

use model::snapshots::Snapshot;
use rune::{El, Tx, col, icon, row, text, text_input};
use theme::Tokens;
use ui::overlay::Modal;
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::snapshots::{CREATE, DELETE, RESTORE};

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

fn camera_button(t: &Tokens, label: &str, phone: bool) -> El {
    let size = if phone { ButtonSize::Touch } else { ButtonSize::Sm };
    let mut button = Button::new(label).size(size).icon(glyph("Camera")).build(t);
    if phone {
        button = button.px(12.0);
    }
    button.on_press(|s: &mut State| s.open_create_snapshot())
}

/// The page, in place of the workspace's main area.
pub fn page(s: &State, t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let project = s.active_project();
    let dot_color = ui::shell::project_color(t, project.map_or("blue", |p| p.color.as_str()));
    let dot = row().size(12.0, 12.0).radius(6.0).fill(dot_color);
    let list = s.snapshot_list();
    let header = if phone {
        let mut header = row()
            .key("snapshots-header")
            .w_full()
            .h(48.0)
            .px(8.0)
            .gap(8.0)
            .items_center()
            .justify_between()
            .fill(c.card)
            .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
            .child(
                row().min_w(0.0).gap(8.0).items_center().child(dot).child(
                    text("Workspace Snapshots").font(sans(14.0, 600.0, 20.0)).color(c.foreground).role("heading").single_line().truncate(),
                ),
            );
        if !list.is_empty() {
            header = header.child(camera_button(t, "New", true));
        }
        header
    } else {
        let title = row()
            .gap(8.0)
            .items_center()
            .role("heading")
            .child(dot)
            .child(text(project.map_or(String::new(), |p| p.name.clone())).font(sans(18.0, 600.0, 28.0)).color(c.foreground))
            .child(text("/").font(sans(18.0, 400.0, 28.0)).color(c.border).mx(4.0))
            .child(text("Workspace Snapshots").font(sans(18.0, 400.0, 28.0)).color(c.secondary_foreground));
        row()
            .key("snapshots-header")
            .w_full()
            .h(56.0)
            .px(24.0)
            .items_center()
            .justify_between()
            .fill(c.card)
            .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
            .child(title)
            .child(camera_button(t, "Create New Snapshot", false))
    };
    let mut cards = col().key("snapshot-list").w_full().max_w(1024.0).gap(if phone { 12.0 } else { 16.0 });
    if list.is_empty() {
        cards = cards.child(empty(t, phone));
    } else {
        for snapshot in list {
            cards = cards.child(if phone { phone_card(s, t, snapshot) } else { card(s, t, snapshot) });
        }
    }
    col()
        .key("snapshots-page")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .border_sides([0.0, 0.0, 0.0, if phone { 0.0 } else { 1.0 }], c.border)
        .child(header)
        .child(
            col()
                .key("snapshots-scroll")
                .grow(1.0)
                .w_full()
                .min_h(0.0)
                .fill(c.terminal_bg)
                .child(col().w_full().padding(if phone { 8.0 } else { 24.0 }).items_center().child(cards))
                .scroll(rune::ScrollSpec { stick_bottom: false }),
        )
}

fn empty(t: &Tokens, phone: bool) -> El {
    let c = &t.colors;
    col()
        .key("snapshots-empty")
        .w_full()
        .py(64.0)
        .px(if phone { 8.0 } else { 0.0 })
        .items_center()
        .child(icon(glyph("Camera"), 48.0).color(c.muted_foreground.alpha(0.5)).mb(16.0))
        .child(text("No snapshots yet").font(sans(18.0, 500.0, 28.0)).color(c.foreground).role("heading").mb(8.0))
        .child(
            text("Create a snapshot to save your current workspace state")
                .font(sans(14.0, 400.0, 20.0))
                .color(c.muted_foreground)
                .mb(16.0),
        )
        .child(camera_button(t, "Create First Snapshot", phone))
}

/// The terminal-ish thumbnail (`SnapshotThumbnail`): four bars whose colours follow the tag.
fn thumbnail(t: &Tokens, snapshot: &Snapshot) -> El {
    let c = &t.colors;
    let lines = if snapshot.tag.as_deref() == Some("stable") {
        [(c.success.alpha(0.5), 0.75), (c.muted.alpha(0.5), 0.5), (c.muted.alpha(0.5), 0.66), (c.primary.alpha(0.3), 1.0)]
    } else if snapshot.process_count == 0 {
        [(c.success.alpha(0.5), 0.3), (c.muted.alpha(0.2), 1.0), (c.muted.alpha(0.2), 0.75), (c.success.alpha(0.5), 0.3)]
    } else {
        [(c.destructive.alpha(0.8), 0.25), (c.destructive.alpha(0.4), 0.75), (c.destructive.alpha(0.4), 0.5), (c.muted.alpha(0.3), 0.66)]
    };
    let mut bars = col().w_full().gap(2.0);
    for (color, width) in lines {
        bars = bars.child(row().w(150.0 * width).h(2.0).radius(1.0).fill(color).mb(2.0));
    }
    col()
        .size(160.0, 96.0)
        .padding(4.0)
        .radius(4.0)
        .clip_rounded(4.0)
        .fill(c.terminal_bg)
        .border(1.0, c.border)
        .a11y_hidden()
        .child(bars)
}

fn tag(t: &Tokens, name: &str) -> El {
    let c = &t.colors;
    let (fill, ink, edge) = if name == "stable" {
        (c.success.alpha(0.15), c.success, c.success.alpha(0.4))
    } else {
        (c.primary.alpha(0.1), c.primary, c.primary.alpha(0.3))
    };
    text(name.to_uppercase())
        .font(sans(10.0, 700.0, 14.0))
        .color(ink)
        .px(8.0)
        .py(2.0)
        .radius(4.0)
        .fill(fill)
        .border(1.0, edge)
}

fn meta(s: &State, t: &Tokens, snapshot: &Snapshot, gap: f32) -> El {
    let c = &t.colors;
    let created = crate::git_history::parse_iso(&snapshot.created_at).unwrap_or(0.0);
    let age = model::snapshots::age(created, crate::editor_host::epoch_ms());
    let _ = s;
    let item = |name: &str, label: String| {
        row()
            .gap(6.0)
            .items_center()
            .child(icon(glyph(name), 14.0).color(c.muted_foreground).a11y_hidden())
            .child(text(label).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground))
    };
    row()
        .gap(gap)
        .items_center()
        .child(item("Clock", age))
        .child(item("Cpu", format!("{} Active Processes", snapshot.process_count)))
        .child(item("Grid3X3", format!("{} Panes", snapshot.pane_count)))
}

fn icon_button(t: &Tokens, name: &str, label: &str, key: String, shown: bool) -> El {
    let c = &t.colors;
    let ink = if shown { c.muted_foreground } else { c.muted_foreground.alpha(0.0) };
    row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .padding(6.0)
        .radius(4.0)
        .hover(c.secondary)
        .child(icon(glyph(name), 14.0).color(ink))
}

fn card(s: &State, t: &Tokens, snapshot: &Snapshot) -> El {
    let c = &t.colors;
    let hovered = s.snapshots.hovered.as_deref() == Some(snapshot.id.as_str());
    let (restore_pick, delete_pick, hover_id) = (snapshot.clone(), snapshot.clone(), snapshot.id.clone());
    let mut name_row = row().gap(12.0).items_center().child(
        text(snapshot.name.clone()).font(sans(16.0, 600.0, 24.0)).color(c.foreground).role("heading"),
    );
    if let Some(name) = &snapshot.tag {
        name_row = name_row.child(tag(t, name));
    }
    let actions = row()
        .gap(8.0)
        .items_center()
        .child(icon_button(t, "Edit", "Rename", format!("rename-{}", snapshot.id), hovered))
        .child(
            icon_button(t, "Trash2", "Delete", format!("delete-{}", snapshot.id), hovered)
                .on_press(move |s: &mut State| s.ask_delete(delete_pick.clone())),
        );
    let content = col()
        .grow(1.0)
        .min_w(0.0)
        .pt(4.0)
        .child(row().w_full().items_center().justify_between().mb(4.0).child(name_row).child(actions))
        .child(
            text(snapshot.description.clone().unwrap_or_default())
                .font(sans(14.0, 400.0, 20.0))
                .color(c.muted_foreground)
                .single_line()
                .truncate()
                .h(20.0)
                .mb(12.0),
        )
        .child(meta(s, t, snapshot, 24.0));
    let restore = row()
        .key(format!("restore-{}", snapshot.id))
        .role("button")
        .label("Restore")
        .gap(8.0)
        .items_center()
        .px(12.0)
        .py(6.0)
        .radius(4.0)
        .fill(c.card)
        .hover(c.secondary)
        .border(1.0, c.border)
        .child(icon(glyph("RotateCcw"), 14.0).color(c.foreground).a11y_hidden())
        .child(text("Restore").font(sans(12.0, 500.0, 16.0)).color(c.foreground))
        .on_press(move |s: &mut State| s.ask_restore(restore_pick.clone()));
    row()
        .key(format!("snapshot-{}", snapshot.id))
        .w_full()
        .padding(16.0)
        .gap(20.0)
        .radius(8.0)
        .fill(c.card.alpha(0.5))
        .border(1.0, if hovered { c.muted_foreground.alpha(0.5) } else { c.border })
        .child(thumbnail(t, snapshot))
        .child(content)
        .child(
            col()
                .h(64.0)
                .self_center()
                .justify_center()
                .pl(16.0)
                .border_sides([0.0, 0.0, 0.0, 1.0], c.border)
                .child(restore),
        )
        .on_hover_change(move |s: &mut State, on: bool, _tx: &mut Tx| {
            if on {
                s.snapshots.hovered = Some(hover_id.clone());
            } else if s.snapshots.hovered.as_deref() == Some(&hover_id) {
                s.snapshots.hovered = None;
            }
        })
}

fn phone_card(s: &State, t: &Tokens, snapshot: &Snapshot) -> El {
    let c = &t.colors;
    let renaming = s.snapshots.renaming.as_deref() == Some(snapshot.id.as_str());
    let (restore_pick, delete_pick, rename_pick) = (snapshot.clone(), snapshot.clone(), snapshot.clone());
    let mut title = row().w_full().gap(8.0).items_center().mb(4.0);
    if renaming {
        title = title.child(
            text_input(&s.snapshots.rename, sans(16.0, 400.0, 24.0))
                .single_line()
                .key("snapshot-rename")
                .label("Rename snapshot")
                .color(c.foreground)
                .grow(1.0)
                .min_w(0.0)
                .h(44.0)
                .px(8.0)
                .radius(4.0)
                .fill(c.background)
                .border(1.0, c.border)
                .focus_ring(1.0, 0.0, c.focus_ring)
                .autofocus()
                .on_edit(|s: &mut State| &mut s.snapshots.rename)
                .on_submit(|s: &mut State, _tx: &mut Tx| s.confirm_snapshot_rename()),
        );
    } else {
        title = title.child(
            text(snapshot.name.clone())
                .font(sans(16.0, 600.0, 24.0))
                .color(c.foreground)
                .role("heading")
                .single_line()
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );
        if let Some(name) = &snapshot.tag {
            title = title.child(tag(t, name));
        }
    }
    let content = col()
        .grow(1.0)
        .min_w(0.0)
        .child(title)
        .child(
            text(snapshot.description.clone().unwrap_or_default())
                .font(sans(14.0, 400.0, 20.0))
                .color(c.muted_foreground)
                .single_line()
                .truncate()
                .h(20.0)
                .mb(8.0),
        )
        .child(meta(s, t, snapshot, 16.0));
    let touch = |name: &str, label: &str, key: String| {
        Button::icon_only(glyph(name), label, ButtonSize::Touch)
            .variant(ButtonVariant::Ghost)
            .build(t)
            .key(key)
            .w(44.0)
    };
    let mut actions = row().w_full().mt(12.0).gap(8.0).items_center().child(
        Button::new("Restore")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Touch)
            .icon(glyph("RotateCcw"))
            .build(t)
            .key(format!("restore-{}", snapshot.id))
            .grow(1.0)
            .border(1.0, c.border)
            .on_press(move |s: &mut State| s.ask_restore(restore_pick.clone())),
    );
    if !renaming {
        actions = actions.child(
            touch("Edit", "Rename snapshot", format!("rename-{}", snapshot.id))
                .on_press(move |s: &mut State| s.start_snapshot_rename(&rename_pick)),
        );
    }
    actions = actions.child(
        touch("Trash2", "Delete snapshot", format!("delete-{}", snapshot.id))
            .on_press(move |s: &mut State| s.ask_delete(delete_pick.clone())),
    );
    col()
        .key(format!("snapshot-{}", snapshot.id))
        .w_full()
        .padding(12.0)
        .radius(8.0)
        .fill(c.card.alpha(0.5))
        .border(1.0, c.border)
        .child(row().w_full().gap(12.0).child(thumbnail(t, snapshot)).child(content))
        .child(actions)
}

/// The dialog footer's buttons: in a row at the end, or on a phone stacked full width with the
/// confirm on top.
fn footer(modal: Modal, phone: bool, cancel: El, confirm: El) -> Modal {
    if phone {
        modal.footer(col().w_full().gap(8.0).child(confirm.w_full().h(44.0)).child(cancel.w_full().h(44.0)))
    } else {
        modal.footer(cancel).footer(confirm)
    }
}

fn caption(t: &Tokens, label: &str) -> El {
    text(label.to_owned()).font(sans(12.0, 500.0, 16.0)).color(t.colors.muted_foreground).mb(4.0)
}

pub fn dialogs(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) {
    create(s, t, phone, layer);
    restore(s, t, phone, layer);
    delete(s, t, phone, layer);
}

fn create(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) {
    if !s.overlays.is_open(CREATE) {
        return;
    }
    let c = &t.colors;
    let ui = &s.snapshots;
    let field_font = sans(if phone { 16.0 } else { 14.0 }, 400.0, 20.0);
    let name = text_input(&ui.name, field_font)
        .single_line()
        .key("snapshot-name")
        .label("Snapshot Name")
        .placeholder("Pre-deployment state".to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .h(if phone { 44.0 } else { 34.0 })
        .px(12.0)
        .radius(4.0)
        .fill(c.secondary)
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .autofocus()
        .on_edit(|s: &mut State| &mut s.snapshots.name)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.create_snapshot());
    let description = text_input(&ui.description, field_font)
        .key("snapshot-description")
        .label("Description (optional)")
        .placeholder("Describe the state of your workspace...".to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .h(74.0)
        .px(12.0)
        .py(6.0)
        .radius(4.0)
        .fill(c.secondary)
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .on_edit(|s: &mut State| &mut s.snapshots.description);
    let can_create = !ui.name.text.trim().is_empty() && !ui.creating;
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .build(t)
        .key("cancel")
        .on_press(|s: &mut State| s.close(CREATE));
    let confirm = Button::new(if ui.creating { "Creating..." } else { "Create" })
        .size(ButtonSize::Sm)
        .disabled(!can_create)
        .build(t)
        .key("create")
        .on_press(|s: &mut State| s.create_snapshot());
    let modal = Modal::new(CREATE, "Create Snapshot")
        .width(480.0)
        .child(col().w_full().child(caption(t, "Snapshot Name")).child(name))
        .child(col().w_full().child(caption(t, "Description (optional)")).child(description));
    footer(modal, phone, cancel, confirm)
        .open(true)
        .on_close(|s: &mut State| s.close(CREATE))
        .build(t, layer);
}

fn confirm_body(t: &Tokens, verb: &str, name: &str, rest: &str) -> El {
    let c = &t.colors;
    col()
        .w_full()
        .gap(16.0)
        .child(
            row()
                .w_full()
                .flex_wrap()
                .child(text(format!("Are you sure you want to {verb} the snapshot ")).font(sans(14.0, 400.0, 20.0)).color(c.foreground))
                .child(text(format!("\"{name}\"")).font(sans(14.0, 600.0, 20.0)).color(c.foreground))
                .child(text("?").font(sans(14.0, 400.0, 20.0)).color(c.foreground)),
        )
        .child(text(rest.to_owned()).font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).wrap().w_full())
}

fn restore(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) {
    let Some(snapshot) = s.snapshots.restore.as_ref().filter(|_| s.overlays.is_open(RESTORE)) else {
        return;
    };
    let c = &t.colors;
    let busy = s.snapshots.restoring;
    let plural = if snapshot.pane_count == 1 { "" } else { "s" };
    let rest = format!(
        "This will close all current terminals and recreate {} terminal{plural} from the snapshot.",
        snapshot.pane_count
    );
    let mut body = confirm_body(t, "restore", &snapshot.name, &rest);
    if s.has_running_terminals() {
        body = body.child(
            row()
                .key("restore-warning")
                .w_full()
                .gap(8.0)
                .padding(12.0)
                .radius(4.0)
                .fill(c.warning.alpha(0.1))
                .border(1.0, c.warning.alpha(0.5))
                .child(icon(glyph("AlertTriangle"), 16.0).color(c.warning).mt(2.0).a11y_hidden())
                .child(
                    text("Warning: You have terminals with running processes. Restoring will terminate these processes.")
                        .font(sans(14.0, 400.0, 20.0))
                        .color(c.warning)
                        .wrap()
                        .grow(1.0)
                        .min_w(0.0),
                ),
        );
    }
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .disabled(busy)
        .build(t)
        .key("cancel")
        .autofocus()
        .on_press(|s: &mut State| s.close_snapshot_confirm(RESTORE));
    let confirm = Button::new(if busy { "Restoring..." } else { "Restore" })
        .size(ButtonSize::Sm)
        .icon(glyph("RotateCcw"))
        .disabled(busy)
        .build(t)
        .key("confirm-restore")
        .on_press(|s: &mut State| s.confirm_restore());
    let modal = Modal::new(RESTORE, "Restore Snapshot").width(480.0).child(body);
    footer(modal, phone, cancel, confirm)
        .open(true)
        .on_close(|s: &mut State| s.close_snapshot_confirm(RESTORE))
        .build(t, layer);
}

fn delete(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) {
    let Some(snapshot) = s.snapshots.delete.as_ref().filter(|_| s.overlays.is_open(DELETE)) else {
        return;
    };
    let c = &t.colors;
    let busy = s.snapshots.deleting;
    let body = confirm_body(t, "delete", &snapshot.name, "This action cannot be undone.");
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .disabled(busy)
        .build(t)
        .key("cancel")
        .autofocus()
        .on_press(|s: &mut State| s.close_snapshot_confirm(DELETE));
    let confirm = Button::new(if busy { "Deleting..." } else { "Delete" })
        .size(ButtonSize::Sm)
        .icon(glyph("Trash2"))
        .fill(c.destructive_fill)
        .ink(c.destructive_foreground)
        .disabled(busy)
        .build(t)
        .key("confirm-delete")
        .on_press(|s: &mut State| s.confirm_delete());
    let modal = Modal::new(DELETE, "Delete Snapshot").width(480.0).child(body);
    footer(modal, phone, cancel, confirm)
        .open(true)
        .on_close(|s: &mut State| s.close_snapshot_confirm(DELETE))
        .build(t, layer);
}
