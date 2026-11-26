//! The canvas editor at full pane size (`AutomationEditor.tsx`): the draft's header (unsaved
//! count, Discard, Save, Approve), the toolbar (add a step, undo, redo, delete), the editable
//! canvas, the inspector (`TriggerInspector`, `NodeInspector` and the kind forms), the conflict
//! banner and the Issues dock.

use model::automations::format::{self, State as AState};
use model::automations::kinds::{self, Group, group};
use model::automations::{NodeKind, Op, TRIGGER};
use rune::{El, Tx, col, el, icon, row, text};
use theme::Tokens;
use troupe_flow::canvas::{Canvas, FlowEvent, FlowState, Selection};
use ui::automations::{glyph, kind_icon, state_pill};
use ui::overlay::{ConfirmDialog, ConfirmVariant};
use ui::type_::{TEXT_SM, TEXT_XS, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::automation_canvas::{Marks, cards_and_wires, look};
use crate::automation_editor::{DISCARD_CONFIRM, EditorUi};

/// `INSPECTOR_INSET`: the floating inspector's width plus its gap.
const INSPECTOR_W: f32 = 320.0;

fn changes_label(count: usize, phone: bool) -> String {
    match count {
        0 => "No unsaved changes".to_owned(),
        n if phone => format!("{n} unsaved"),
        1 => "Draft · 1 unsaved change".to_owned(),
        n => format!("Draft · {n} unsaved changes"),
    }
}

fn flow_lens(s: &mut State) -> &mut FlowState {
    &mut s
        .automations
        .editor
        .as_mut()
        .expect("the editor is open")
        .flow
}

fn flow_event(s: &mut State, event: FlowEvent, _: &mut Tx) {
    s.automation_flow_event(event);
}

pub fn editor(_s: &State, t: &'static Tokens, ed: &EditorUi, phone: bool) -> El {
    let c = &t.colors;
    let draft = &ed.draft;
    let a = &draft.base;
    let count = draft.pending.len();
    let dirty = count > 0;
    let needs_approval = format::state(a) == AState::NeedsApproval;
    let titles = if phone {
        col()
    } else {
        row().gap(8.0).items_center()
    };
    let mut header = row()
        .w_full()
        .min_h(48.0)
        .px(8.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            Button::icon_only(
                glyph("ArrowLeft"),
                "Close the editor",
                if phone {
                    ButtonSize::Touch
                } else {
                    ButtonSize::IconSm
                },
            )
            .variant(ButtonVariant::Ghost)
            .disabled(ed.saving)
            .build(t)
            .key("automation-editor-close")
            .on_press(|s: &mut State| s.automation_close_editor()),
        )
        .child(
            titles
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text(a.name.clone())
                        .font(sans(14.0, 500.0, 20.0))
                        .color(c.foreground)
                        .truncate(),
                )
                .child(
                    row()
                        .gap(8.0)
                        .items_center()
                        .child(state_pill(t, format::state(a)))
                        .child(
                            text(changes_label(count, phone))
                                .key("automation-draft-status")
                                .font(TEXT_XS)
                                .color(c.muted_foreground),
                        ),
                ),
        );
    if !phone || dirty {
        header = header
            .child(
                Button::new("Discard")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .disabled(!dirty || ed.saving)
                    .build(t)
                    .key("automation-editor-discard")
                    .on_press(|s: &mut State| s.automation_discard()),
            )
            .child(
                Button::new(if ed.saving { "Saving…" } else { "Save" })
                    .variant(if dirty {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    })
                    .size(ButtonSize::Sm)
                    .disabled(!dirty || ed.saving)
                    .build(t)
                    .key("automation-editor-save")
                    .on_press(|s: &mut State| s.automation_save()),
            );
    }
    if needs_approval && (!phone || !dirty) {
        let label = if ed.approving {
            "Approving…"
        } else if a.enabled && !phone {
            "Approve and arm"
        } else {
            "Approve"
        };
        header = header.child(
            Button::new(label)
                .variant(if dirty {
                    ButtonVariant::Outline
                } else {
                    ButtonVariant::Default
                })
                .size(ButtonSize::Sm)
                .disabled(dirty || !draft.issues.is_empty() || ed.approving)
                .build(t)
                .key("automation-editor-approve")
                .on_press(|s: &mut State| s.automation_editor_approve()),
        );
    }

    let marks = Marks {
        issues: &draft.issues,
        run: None,
        selection: ed.flow.selection.as_ref(),
        dimmed: &|_| false,
        now_ms: crate::editor_host::epoch_ms(),
        offset_min: crate::automations_view::offset_min(),
    };
    let (cards, wires) = cards_and_wires(
        t,
        &draft.doc.graph,
        &draft.doc.trigger,
        &draft.layout,
        &marks,
    );
    let canvas = Canvas::new("automation-editor-canvas", flow_lens, flow_event, look(t))
        .editable(!ed.saving)
        .wheel_pans(!phone)
        .readable(if phone { 0.85 } else { 0.75 })
        .cards(cards)
        .wires(wires)
        .build(&ed.flow);

    let mut stage = el()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(canvas)
        .child(toolbar(t, ed));
    if ed.adding {
        stage = stage.child(add_menu(t));
    }
    if let Some(conflict) = &draft.conflict {
        stage = stage.child(conflict_banner(
            t,
            conflict.by.as_deref(),
            &conflict.dropped,
            draft,
        ));
    }
    let inspector = crate::automation_inspector::inspector(t, ed, phone);
    let mut page = col()
        .key("automation-editor")
        .role("group")
        .label(format!("Edit {}", a.name))
        .w_full()
        .h_full()
        .fill(c.background)
        .child(header);
    match (inspector, phone) {
        (Some(panel), false) => {
            stage = stage.child(
                panel
                    .pin(None, Some(12.0), Some(12.0), Some(12.0))
                    .w(INSPECTOR_W),
            );
            page = page.child(stage).child(dock(t, ed, phone));
        }
        (Some(panel), true) => page = page.child(stage).child(panel.w_full().max_h(360.0)),
        (None, _) => page = page.child(stage).child(dock(t, ed, phone)),
    }
    page
}

fn toolbar(t: &Tokens, ed: &EditorUi) -> El {
    let c = &t.colors;
    let deletable = matches!(&ed.flow.selection, Some(Selection::Card(id)) if id != TRIGGER)
        || matches!(ed.flow.selection, Some(Selection::Wire(_)));
    let tool = |name: &str, label: &str, key: &str, disabled: bool| {
        Button::icon_only(glyph(name), label, ButtonSize::IconSm)
            .variant(ButtonVariant::Ghost)
            .disabled(disabled)
            .build(t)
            .key(key.to_owned())
    };
    row()
        .pin(Some(12.0), Some(12.0), None, None)
        .gap(2.0)
        .padding(4.0)
        .radius(10.0)
        .border(1.0, c.border)
        .fill(c.popover)
        .child(
            tool("Plus", "Add a step", "automation-add-step", ed.saving).on_press(
                |s: &mut State| {
                    if let Some(editor) = s.automations.editor.as_mut() {
                        editor.adding = !editor.adding;
                    }
                },
            ),
        )
        .child(
            tool(
                "Undo2",
                "Undo",
                "automation-undo",
                ed.saving || ed.draft.undo.is_empty(),
            )
            .on_press(|s: &mut State| s.automation_undo()),
        )
        .child(
            tool(
                "Redo2",
                "Redo",
                "automation-redo",
                ed.saving || ed.draft.redo.is_empty(),
            )
            .on_press(|s: &mut State| s.automation_redo()),
        )
        .child(
            tool(
                "Trash2",
                "Delete the selection",
                "automation-delete-selection",
                ed.saving || !deletable,
            )
            .on_press(|s: &mut State| s.automation_delete_selected()),
        )
}

/// The add menu (`AddNodeMenu`): the kinds by group.
fn add_menu(t: &Tokens) -> El {
    let c = &t.colors;
    let mut menu = col()
        .key("automation-add-menu")
        .role("menu")
        .label("Add a step")
        .pin(Some(12.0), Some(56.0), None, None)
        .w(220.0)
        .padding(4.0)
        .radius(10.0)
        .border(1.0, c.border)
        .fill(c.popover);
    for g in Group::ALL {
        menu = menu.child(
            text(g.label())
                .font(sans(12.0, 500.0, 16.0))
                .color(c.muted_foreground)
                .px(8.0)
                .py(4.0),
        );
        for kind in NodeKind::ALL.into_iter().filter(|k| group(*k) == g) {
            menu = menu.child(
                row()
                    .key(format!("automation-add-{}", kind.as_str()))
                    .role("menuitem")
                    .label(kinds::label(kind))
                    .h(32.0)
                    .px(8.0)
                    .gap(8.0)
                    .radius(6.0)
                    .items_center()
                    .hover(c.foreground.alpha(0.05))
                    .child(icon(kind_icon(kind), 14.0).color(c.secondary_foreground))
                    .child(text(kinds::label(kind)).font(TEXT_SM).color(c.foreground))
                    .on_press(move |s: &mut State| s.automation_add(kind)),
            );
        }
    }
    menu
}

fn describe(op: &Op, label: &dyn Fn(&str) -> String) -> String {
    let edge = |e: &model::automations::Edge| {
        let port = if e.port == model::automations::Port::Out {
            String::new()
        } else {
            format!(" ({})", e.port.as_str())
        };
        format!("{} → {}{port}", label(&e.from), label(&e.to))
    };
    match op {
        Op::AddNode { node } => format!("Add {}", node.label),
        Op::UpdateNode { id, .. } => format!("Edit {}", label(id)),
        Op::RemoveNode { id } => format!("Delete {}", label(id)),
        Op::Connect { edge: e } => format!("Connect {}", edge(e)),
        Op::Disconnect { edge: e } => format!("Disconnect {}", edge(e)),
        Op::SetTrigger { .. } => "Change the trigger".to_owned(),
        Op::Rename { name } => format!("Rename to {name}"),
        Op::SetIsolation { .. } => "Change the isolation".to_owned(),
    }
}

fn conflict_banner(
    t: &Tokens,
    by: Option<&str>,
    dropped: &[Op],
    draft: &model::automations::draft::Draft,
) -> El {
    let c = &t.colors;
    let label = |id: &str| {
        if id == TRIGGER {
            return "Trigger".to_owned();
        }
        draft
            .doc
            .graph
            .node(id)
            .or_else(|| draft.base.graph.node(id))
            .map_or_else(|| id.to_owned(), |n| n.label.clone())
    };
    let mut body = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(4.0)
        .child(
            text(format!(
                "Updated by {} while you edited",
                by.unwrap_or("someone else")
            ))
            .font(sans(14.0, 500.0, 20.0))
            .color(c.warning),
        )
        .child(
            text("Your edits now sit on the latest version. Review them, then Save.")
                .font(TEXT_XS)
                .color(c.muted_foreground),
        );
    if !dropped.is_empty() {
        let n = dropped.len();
        body = body.child(
            text(format!(
                "{} no longer apply:",
                if n == 1 {
                    "1 edit".to_owned()
                } else {
                    format!("{n} edits")
                }
            ))
            .font(TEXT_XS)
            .color(c.foreground),
        );
        body = body.children(dropped.iter().map(|op| {
            text(format!("• {}", describe(op, &label)))
                .font(TEXT_XS)
                .color(c.muted_foreground)
        }));
    }
    row()
        .key("automation-conflict")
        .role("status")
        .pin(Some(12.0), Some(12.0), Some(12.0), None)
        .max_w(576.0)
        .px(16.0)
        .py(12.0)
        .gap(12.0)
        .radius(12.0)
        .border(1.0, c.border)
        .fill(c.popover)
        .child(body)
        .child(
            Button::icon_only(glyph("X"), "Dismiss", ButtonSize::IconSm)
                .variant(ButtonVariant::Ghost)
                .build(t)
                .key("automation-conflict-dismiss")
                .on_press(|s: &mut State| {
                    if let Some(editor) = s.automations.editor.as_mut() {
                        editor.draft = model::automations::draft::reduce(
                            &editor.draft,
                            model::automations::draft::Action::DismissConflict,
                        );
                    }
                }),
        )
}

fn dock(t: &Tokens, ed: &EditorUi, phone: bool) -> El {
    let c = &t.colors;
    let refusal = ed.refusal();
    let count = ed.draft.issues.len() + usize::from(refusal.is_some());
    let expanded = ed.dock_open && count > 0;
    let mut section = col()
        .key("automation-dock")
        .role("region")
        .label("Dock")
        .w_full()
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
        .child(
            row().w_full().px(8.0).py(6.0).items_center().child(
                Button::new(format!("Issues ({count})"))
                    .variant(ButtonVariant::Ghost)
                    .size(if phone {
                        ButtonSize::Touch
                    } else {
                        ButtonSize::Sm
                    })
                    .icon(glyph("AlertCircle"))
                    .disabled(count == 0)
                    .build(t)
                    .key("automation-issues-toggle")
                    .on_press(|s: &mut State| {
                        if let Some(editor) = s.automations.editor.as_mut() {
                            editor.dock_open = !editor.dock_open;
                        }
                    }),
            ),
        );
    if !expanded {
        return section;
    }
    let label = |id: &str| {
        ed.draft
            .doc
            .graph
            .node(id)
            .map_or_else(|| id.to_owned(), |n| n.label.clone())
    };
    let mut list = col().w_full().max_h(192.0).px(12.0).pb(12.0).gap(4.0);
    if let Some(refusal) = refusal {
        list = list.child(
            row()
                .w_full()
                .px(12.0)
                .py(8.0)
                .gap(8.0)
                .radius(8.0)
                .fill(c.destructive.alpha(0.1))
                .child(icon(glyph("AlertCircle"), 14.0).color(c.destructive))
                .child(
                    text(format!("The host refuses these edits: {refusal}"))
                        .font(TEXT_XS)
                        .color(c.destructive)
                        .grow(1.0),
                ),
        );
    }
    for issue in &ed.draft.issues {
        let target = issue.node_id.clone().unwrap_or_else(|| TRIGGER.to_owned());
        let lead = issue
            .node_id
            .as_deref()
            .map(|id| format!("{}: ", label(id)))
            .unwrap_or_default();
        list = list.child(
            row()
                .w_full()
                .px(12.0)
                .py(6.0)
                .gap(8.0)
                .radius(8.0)
                .items_center()
                .fill(c.destructive.alpha(0.1))
                .child(icon(glyph("AlertCircle"), 14.0).color(c.destructive))
                .child(
                    text(format!("{lead}{}", issue.message))
                        .font(TEXT_XS)
                        .color(c.destructive)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    Button::new("Show")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Xs)
                        .build(t)
                        .on_press(move |s: &mut State| {
                            if let Some(editor) = s.automations.editor.as_mut() {
                                editor.flow.selection = Some(Selection::Card(target.clone()));
                            }
                            s.automation_inspect(Some(target.clone()));
                        }),
                ),
        );
    }
    section = section.child(list);
    section
}

/// The discard confirm (`AutomationEditor`'s `ConfirmDialog`).
pub fn dialogs(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    if !s.overlays.is_open(DISCARD_CONFIRM) {
        return;
    }
    let count = s
        .automations
        .editor
        .as_ref()
        .map_or(0, |e| e.draft.pending.len());
    let changes = if count == 1 {
        "1 unsaved change".to_owned()
    } else {
        format!("{count} unsaved changes")
    };
    ConfirmDialog::new(
        DISCARD_CONFIRM,
        "Discard unsaved changes",
        format!("Close the editor and discard {changes}?"),
    )
    .variant(ConfirmVariant::Danger)
    .confirm_label("Discard")
    .open(true)
    .on_cancel(|s: &mut State| s.close(DISCARD_CONFIRM))
    .on_confirm(|s: &mut State| s.automation_discard_and_close())
    .build(t, layer);
}
