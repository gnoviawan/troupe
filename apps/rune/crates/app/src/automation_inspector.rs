//! The inspector (`TriggerInspector.tsx`, `NodeInspector.tsx` and the kind forms in
//! `node-forms.tsx`, `AgentForm.tsx`, `McpToolForm.tsx`): a card's name and its kind's fields,
//! the choices that change a spec at once, Apply, Delete step and Close.

use model::automations::format;
use model::automations::kinds;
use model::automations::{
    ConditionOp, HttpMethod, MergeMode, NodeSpec, Schedule, TRIGGER, Trigger,
};
use rune::{El, Tx, col, row, text, text_input};
use theme::Tokens;
use troupe_flow::canvas::Selection;
use ui::automations::glyph;
use ui::type_::{TEXT_SM, TEXT_XS, sans};
use ui::{Button, ButtonSize, ButtonVariant, Input};

use crate::State;
use crate::automation_editor::EditorUi;
use crate::automation_fields::{set_method, set_mode, set_op, trigger_of};

fn field(
    t: &Tokens,
    label: &str,
    edit: &rune::TextEdit,
    key: &str,
    lens: fn(&mut State) -> &mut rune::TextEdit,
) -> El {
    col()
        .w_full()
        .gap(4.0)
        .child(
            text(label.to_owned())
                .font(sans(12.0, 500.0, 16.0))
                .color(t.colors.muted_foreground),
        )
        .child(
            Input::new(edit)
                .label(label.to_owned())
                .build(t)
                .key(key.to_owned())
                .on_edit(lens)
                .on_submit(|s: &mut State, _tx: &mut Tx| s.automation_commit_fields()),
        )
}

fn area(
    t: &Tokens,
    label: &str,
    edit: &rune::TextEdit,
    key: &str,
    lens: fn(&mut State) -> &mut rune::TextEdit,
) -> El {
    let c = &t.colors;
    col()
        .w_full()
        .gap(4.0)
        .child(
            text(label.to_owned())
                .font(sans(12.0, 500.0, 16.0))
                .color(c.muted_foreground),
        )
        .child(
            text_input(edit, TEXT_SM)
                .key(key.to_owned())
                .label(label.to_owned())
                .w_full()
                .min_h(80.0)
                .padding(8.0)
                .radius(6.0)
                .border(1.0, c.field_border)
                .color(c.foreground)
                .on_edit(lens),
        )
}

fn fields(s: &mut State) -> &mut crate::automation_fields::Fields {
    &mut s
        .automations
        .editor
        .as_mut()
        .expect("the editor is open")
        .fields
}
fn lens_label(s: &mut State) -> &mut rune::TextEdit {
    &mut fields(s).label
}
fn lens_a(s: &mut State) -> &mut rune::TextEdit {
    &mut fields(s).a
}
fn lens_b(s: &mut State) -> &mut rune::TextEdit {
    &mut fields(s).b
}
fn lens_c(s: &mut State) -> &mut rune::TextEdit {
    &mut fields(s).c
}

/// A row of choices; the chosen one is the keycap.
fn choices<T: Copy + PartialEq + 'static>(
    t: &Tokens,
    key: &str,
    items: &[(T, &str)],
    chosen: T,
    pick: fn(&mut State, T),
) -> El {
    let c = &t.colors;
    row()
        .w_full()
        .gap(4.0)
        .wrap()
        .children(items.iter().map(|(value, label)| {
            let value = *value;
            row()
                .key(format!("{key}-{label}"))
                .role("button")
                .label((*label).to_owned())
                .h(28.0)
                .px(10.0)
                .radius(6.0)
                .items_center()
                .border(1.0, c.border)
                .fill(if value == chosen {
                    c.keycap_fill
                } else {
                    c.background
                })
                .child(text((*label).to_owned()).font(TEXT_XS).color(c.foreground))
                .on_press(move |s: &mut State| pick(s, value))
        }))
}

fn inspected_id(s: &State) -> String {
    s.automations
        .editor
        .as_ref()
        .and_then(|e| e.inspected.clone())
        .unwrap_or_default()
}

pub(crate) fn inspector(t: &'static Tokens, ed: &EditorUi, phone: bool) -> Option<El> {
    let c = &t.colors;
    let id = ed.inspected.as_deref()?;
    let f = &ed.fields;
    let issues: Vec<&model::automations::Issue> = ed
        .draft
        .issues
        .iter()
        .filter(|i| i.node_id.as_deref() == if id == TRIGGER { None } else { Some(id) })
        .collect();
    let (title, mut body) = if id == TRIGGER {
        let kind = match &ed.draft.doc.trigger {
            Trigger::Manual => "manual",
            Trigger::Schedule {
                schedule: Schedule::Every { .. },
            } => "every",
            Trigger::Schedule {
                schedule: Schedule::Cron { .. },
            } => "cron",
            Trigger::Schedule {
                schedule: Schedule::At { .. },
            } => "at",
        };
        let mut body = col().w_full().gap(12.0).child(choices(
            t,
            "automation-trigger",
            &[("manual", "Manual"), ("every", "Every"), ("cron", "Cron")],
            kind,
            |s, k| s.automation_set_trigger(trigger_of(k)),
        ));
        match kind {
            "every" => {
                body = body.child(field(
                    t,
                    "Every (minutes)",
                    &f.a,
                    "automation-field-a",
                    lens_a,
                ))
            }
            "cron" => {
                body = body
                    .child(field(
                        t,
                        "Cron expression",
                        &f.a,
                        "automation-field-a",
                        lens_a,
                    ))
                    .child(field(t, "Time zone", &f.b, "automation-field-b", lens_b))
            }
            "at" => {
                body = body.child(
                    text(format::trigger_summary(
                        &ed.draft.doc.trigger,
                        crate::editor_host::epoch_ms(),
                        crate::automations_view::offset_min(),
                    ))
                    .font(TEXT_SM)
                    .color(c.foreground),
                )
            }
            _ => {
                body = body.child(
                    text("Runs when you press Run now.")
                        .font(TEXT_XS)
                        .color(c.muted_foreground),
                )
            }
        }
        ("Trigger".to_owned(), body)
    } else {
        let node = ed.draft.doc.graph.node(id)?;
        let mut body = col().w_full().gap(12.0).child(field(
            t,
            "Name",
            &f.label,
            "automation-field-label",
            lens_label,
        ));
        body = match &node.spec {
            NodeSpec::Http { method, .. } => body
                .child(choices(
                    t,
                    "automation-method",
                    &HttpMethod::ALL.map(|m| (m, m.as_str())),
                    *method,
                    |s, m| {
                        let id = inspected_id(s);
                        s.automation_set_spec(&id, |spec| set_method(spec, m));
                    },
                ))
                .child(field(t, "URL", &f.a, "automation-field-a", lens_a))
                .child(field(
                    t,
                    "Allowed hosts",
                    &f.b,
                    "automation-field-b",
                    lens_b,
                ))
                .child(area(t, "Body", &f.c, "automation-field-c", lens_c)),
            NodeSpec::Agent { .. } => body
                .child(field(t, "Agent", &f.a, "automation-field-a", lens_a))
                .child(field(t, "Model", &f.c, "automation-field-c", lens_c))
                .child(area(t, "Prompt", &f.b, "automation-field-b", lens_b)),
            NodeSpec::Condition { op, .. } => body
                .child(field(t, "Left", &f.a, "automation-field-a", lens_a))
                .child(choices(
                    t,
                    "automation-op",
                    &ConditionOp::ALL.map(|o| (o, o.as_str())),
                    *op,
                    |s, o| {
                        let id = inspected_id(s);
                        s.automation_set_spec(&id, |spec| set_op(spec, o));
                    },
                ))
                .child(field(t, "Right", &f.b, "automation-field-b", lens_b)),
            NodeSpec::Fail { .. } => {
                body.child(field(t, "Message", &f.a, "automation-field-a", lens_a))
            }
            NodeSpec::ForEach { .. } => body
                .child(field(t, "Items", &f.a, "automation-field-a", lens_a))
                .child(field(
                    t,
                    "Max iterations",
                    &f.b,
                    "automation-field-b",
                    lens_b,
                )),
            NodeSpec::Merge { mode } => body.child(choices(
                t,
                "automation-merge",
                &[
                    (MergeMode::Any, "Any branch"),
                    (MergeMode::All, "Every branch"),
                ],
                *mode,
                |s, m| {
                    let id = inspected_id(s);
                    s.automation_set_spec(&id, |spec| set_mode(spec, m));
                },
            )),
            NodeSpec::Wait { .. } => body.child(field(
                t,
                "Wait (seconds)",
                &f.a,
                "automation-field-a",
                lens_a,
            )),
            NodeSpec::Approval { .. } => body
                .child(area(t, "Message", &f.a, "automation-field-a", lens_a))
                .child(field(
                    t,
                    "Times out after (hours)",
                    &f.b,
                    "automation-field-b",
                    lens_b,
                )),
            NodeSpec::McpTool { .. } => body
                .child(field(t, "Server", &f.a, "automation-field-a", lens_a))
                .child(field(t, "Tool", &f.b, "automation-field-b", lens_b)),
            NodeSpec::Transform { fields } => body.child(
                text(format!(
                    "{} fields. Editing fields is not in the Rune client yet.",
                    fields.len()
                ))
                .font(TEXT_XS)
                .color(c.muted_foreground),
            ),
        };
        body = body.child(
            Button::new("Apply")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .disabled(ed.saving)
                .build(t)
                .key("automation-inspector-apply")
                .on_press(|s: &mut State| s.automation_commit_fields()),
        );
        (
            format!("{} · {}", node.label, kinds::label(node.spec.kind())),
            body,
        )
    };
    if !issues.is_empty() {
        body = body.children(
            issues
                .into_iter()
                .map(|i| text(i.message.clone()).font(TEXT_XS).color(c.destructive)),
        );
    }
    let mut head = row().w_full().gap(4.0).items_center().child(
        text(title)
            .font(sans(14.0, 500.0, 20.0))
            .color(c.foreground)
            .truncate()
            .grow(1.0)
            .min_w(0.0),
    );
    if id != TRIGGER {
        head = head.child(
            Button::icon_only(glyph("Trash2"), "Delete step", ButtonSize::IconSm)
                .variant(ButtonVariant::Ghost)
                .disabled(ed.saving)
                .build(t)
                .key("automation-inspector-delete")
                .on_press(|s: &mut State| {
                    let id = inspected_id(s);
                    if let Some(editor) = s.automations.editor.as_mut() {
                        editor.flow.selection = Some(Selection::Card(id));
                    }
                    s.automation_delete_selected();
                }),
        );
    }
    head = head.child(
        Button::icon_only(
            glyph("X"),
            "Close",
            if phone {
                ButtonSize::Touch
            } else {
                ButtonSize::IconSm
            },
        )
        .variant(ButtonVariant::Ghost)
        .build(t)
        .key("automation-inspector-close")
        .on_press(|s: &mut State| {
            s.automation_commit_fields();
            s.automation_inspect(None);
        }),
    );
    Some(
        col()
            .key("automation-inspector")
            .role("region")
            .label("Inspector")
            .padding(12.0)
            .gap(12.0)
            .radius(12.0)
            .border(1.0, c.border)
            .fill(c.popover)
            .child(head)
            .child(
                ui::base::scroll_area::scroll_area(t, "automation-inspector-scroll", body)
                    .grow(1.0)
                    .min_h(0.0),
            ),
    )
}
