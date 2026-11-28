//! The canvas tab (`CanvasPanel.tsx`): a header (document name, unsaved marker, mount status, save
//! error, Save) over the editor area, with the conflict bar and the crash notice between them
//! (`CanvasRecovery.tsx`), and the unmount confirm (`CanvasUnmountDialog.tsx`). The editor area
//! is the caller's: the desktop app's effect slot, or the box the web client's iframe covers.

use rune::{El, col, el, icon, row, text};
use theme::Tokens;
use theme::icons as i;

use super::flow::Canvases;
use super::session::{Choice, Phase, Remount, Session, UnloadReason};
use crate::overlay::ConfirmDialog;
use crate::type_::sans;
use crate::{Button, ButtonSize, ButtonVariant};

type Get<S> = fn(&S) -> &Canvases;
type Lens<S> = fn(&mut S) -> &mut Canvases;

/// The phone refusal (`canvas-unsupported-surface`).
pub const PHONE_REFUSAL: &str = "The canvas is not available at this screen width.";

fn status_label(phase: &Phase, closing: bool) -> &'static str {
    if closing {
        return "Saving before close…";
    }
    match phase {
        Phase::Opening => "Starting canvas…",
        Phase::Loading { .. } => "Loading editor…",
        Phase::Ready { .. } => "Ready",
        Phase::Failed { .. } => "Canvas unavailable",
        Phase::Elsewhere => "Open in another window",
        Phase::Unloaded { .. } => "Unloaded",
    }
}

fn notice(t: &Tokens, key: &str, words: &str) -> El {
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .padding(16.0)
        .fill(t.colors.background)
        .child(text(words).font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground))
}

/// The tab's pane. `slot` draws the editor area for the mount it is given.
#[allow(clippy::too_many_arguments)]
pub fn panel<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, project_id: &str, doc_path: &str, phone: bool, slot: impl FnOnce(u64) -> El) -> El {
    if phone {
        return notice(t, "canvas-unsupported-surface", PHONE_REFUSAL);
    }
    let Some(session) = get(s).session(project_id) else {
        return notice(t, "canvas-closed", "Canvas closed");
    };
    let c = &t.colors;
    let path = if session.doc_path.is_empty() { doc_path } else { &session.doc_path };
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
    let mut header = row()
        .key("canvas-header")
        .w_full()
        .h(36.0)
        .px(12.0)
        .gap(8.0)
        .items_center()
        .fill(c.card)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(icon(&i::CANVAS, 12.0).color(c.primary).a11y_hidden())
        .child(text(name).font(sans(12.0, 500.0, 16.0)).color(c.foreground).min_w(0.0));
    if session.dirty {
        header = header.child(el().key("canvas-dirty").size(8.0, 8.0).radius(4.0).fill(c.primary_fill).label("Unsaved changes"));
    }
    header = header.child(text(status_label(&session.phase, session.closing)).key("canvas-status").font(sans(11.0, 400.0, 14.0)).color(c.muted_foreground));
    if session.save_error.is_some() {
        header = header.child(text("Save failed").key("canvas-save-error").font(sans(11.0, 400.0, 14.0)).color(c.destructive));
    }
    let ready = matches!(session.phase, Phase::Ready { .. });
    let save_project = project_id.to_owned();
    header = header.child(el().grow(1.0)).child(
        Button::new("Save")
            .icon(if session.saving > 0 { &i::LOADER2 } else { &i::SAVE })
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Xs)
            .disabled(!ready || session.saving > 0)
            .build(t)
            .key("canvas-save")
            .on_press(move |s: &mut S| lens(s).save(&save_project)),
    );
    let mut pane = col().key(format!("canvas-panel-{project_id}")).w_full().grow(1.0).min_h(0.0).fill(c.background).child(header);
    if let Some(conflict) = &session.conflict {
        pane = pane.child(conflict_bar(t, lens, project_id, conflict.disk_hash.is_none(), session.resolving));
    }
    if let Some(remount) = &session.crash {
        pane = pane.child(crash_notice(t, lens, project_id, remount));
    }
    pane.child(body(t, lens, session, project_id, path, slot))
}

fn body<S: 'static>(t: &'static Tokens, lens: Lens<S>, session: &Session, project_id: &str, doc_path: &str, slot: impl FnOnce(u64) -> El) -> El {
    let reopen = |label: &str, key: &str, words: &str| {
        let (project, path) = (project_id.to_owned(), doc_path.to_owned());
        col()
            .key(key)
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .gap(12.0)
            .padding(16.0)
            .fill(t.colors.background)
            .child(text(words).font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground))
            .child(Button::new(label).variant(ButtonVariant::Secondary).size(ButtonSize::Sm).build(t).on_press(move |s: &mut S| lens(s).open(&project, &path, true, 0.0)))
    };
    match &session.phase {
        Phase::Loading { info, .. } | Phase::Ready { info } => col().key("canvas-area").w_full().grow(1.0).min_h(0.0).child(slot(info.mount_seq)),
        Phase::Failed { message, .. } => notice(t, "canvas-mount-failed", message),
        Phase::Elsewhere => reopen("Take back", "canvas-open-elsewhere", "This canvas is open in another window."),
        Phase::Unloaded { reason: UnloadReason::Evicted, .. } => reopen("Reopen canvas", "canvas-unloaded", "This canvas was unloaded to save memory. Click to reopen."),
        Phase::Unloaded { message, .. } => reopen("Try again", "canvas-unloaded", message.as_deref().unwrap_or("Another canvas could not be unloaded to make room.")),
        Phase::Opening => notice(t, "canvas-starting", "Starting canvas…"),
    }
}

fn strip(t: &Tokens, key: &str, glyph: &'static rune::Icon, ink: rune::Color, words: String) -> El {
    row()
        .key(key)
        .w_full()
        .px(12.0)
        .py(8.0)
        .gap(8.0)
        .items_center()
        .fill(t.colors.card)
        .border_sides([0.0, 0.0, 1.0, 0.0], t.colors.border)
        .child(icon(glyph, 14.0).color(ink).a11y_hidden())
        .child(text(words).font(sans(12.0, 400.0, 16.0)).color(t.colors.foreground).grow(1.0).min_w(0.0))
}

/// `CanvasConflictBar`: a deleted document can only be kept (recreated).
fn conflict_bar<S: 'static>(t: &'static Tokens, lens: Lens<S>, project_id: &str, deleted: bool, resolving: bool) -> El {
    let words = if deleted {
        "This document was deleted outside Troupe. Keep your version to recreate it."
    } else {
        "This document changed outside Troupe while you had unsaved changes. Autosave is paused."
    };
    let mut bar = strip(t, "canvas-conflict", &i::ALERT_TRIANGLE, t.colors.warning, words.into());
    let choices = [(Choice::KeepMine, "Keep mine", true), (Choice::TakeDisk, "Take disk", false), (Choice::SaveCopy, "Save a copy", false)];
    for (choice, label, kept_when_deleted) in choices {
        if deleted && !kept_when_deleted {
            continue;
        }
        let project = project_id.to_owned();
        let variant = if choice == Choice::KeepMine { ButtonVariant::Default } else { ButtonVariant::Secondary };
        bar = bar.child(
            Button::new(label)
                .variant(variant)
                .size(ButtonSize::Xs)
                .disabled(resolving)
                .build(t)
                .key(format!("canvas-conflict-{}", choice.wire()))
                .on_press(move |s: &mut S| lens(s).resolve_conflict(&project, choice)),
        );
    }
    bar
}

/// `lostText`: what the crash remount lost.
pub fn lost_text(remount: &Remount) -> String {
    let lead = "The canvas editor stopped responding and was reloaded from the last save.";
    let Some(lost) = &remount.lost else { return lead.into() };
    let changes = if lost.count == 1 { "1 unsaved change".to_owned() } else { format!("{} unsaved changes", lost.count) };
    let verb = if lost.count == 1 { "was" } else { "were" };
    format!("{lead} {changes} {verb} lost.")
}

fn crash_notice<S: 'static>(t: &'static Tokens, lens: Lens<S>, project_id: &str, remount: &Remount) -> El {
    let project = project_id.to_owned();
    strip(t, "canvas-crash-notice", &i::ALERT_TRIANGLE, t.colors.destructive, lost_text(remount)).child(
        crate::IconActionButton::new(&i::X, "Dismiss").build(t).key("canvas-crash-dismiss").on_press(move |s: &mut S| lens(s).dismiss_crash(&project)),
    )
}

/// The unmount confirm for each canvas whose save failed before it could close.
pub fn unmount_confirms<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, layer: &mut Vec<El>) {
    for session in get(s).sessions.values() {
        let Some(message) = &session.unsaved_close else { continue };
        let name = session.doc_path.rsplit(['/', '\\']).next().unwrap_or(&session.doc_path);
        let (retry, discard, keep) = (session.project_id.clone(), session.project_id.clone(), session.project_id.clone());
        ConfirmDialog::new(format!("canvas-unmount-{}", session.project_id), "Canvas not saved", format!("\"{name}\" could not be saved before closing: {message}"))
            .confirm_label("Retry")
            .cancel_label("Keep editing")
            .secondary("Discard changes", move |s: &mut S| lens(s).unmount(&discard, true))
            .on_confirm(move |s: &mut S| lens(s).unmount(&retry, false))
            .on_cancel(move |s: &mut S| lens(s).keep_editing(&keep))
            .open(true)
            .build(t, layer);
    }
}
