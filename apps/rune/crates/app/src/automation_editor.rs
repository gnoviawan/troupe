//! The canvas editor's draft driver (`canvas/draft/use-draft.ts`, `AutomationEditor.tsx`):
//! gestures apply to `model::automations::draft` at once, a dry run (`preview_ops`) 250 ms
//! after the last one brings the host's issues, and Save sends every pending op as one
//! `apply_ops` on the draft's base revision, then the moved positions through `set_layout`.
//! A Save or a dry run that meets a newer head replays the draft on it.

use std::collections::BTreeMap;

use client::http::HttpError;
use model::automations::draft::{self, Action, Draft, LayoutPatch};
use model::automations::gestures::{AddPlacement, add_gesture, move_patch, remove_gesture};
use model::automations::kinds::{chain_port, new_node};
use model::automations::ops::refuse_connect;
use model::automations::routes::Call;
use model::automations::{
    ApplyOpsResult, Automation, Detail, Edge, NodeKind, NodePosition, NodeSpec, Op, Port, Schedule,
    TRIGGER, Trigger,
};
use serde_json::Value;
use troupe_flow::canvas::{FlowEvent, FlowState, Selection};

use crate::State;
use crate::automation_fields::{Fields, fields_of, spec_from};
use crate::automations::{AutoFetch, decode};

/// A dry run waits for the gesture burst to settle (`PREVIEW_DELAY_MS`).
const PREVIEW_DELAY: f64 = 0.25;
pub const DISCARD_CONFIRM: &str = "automation-discard-confirm";
/// What to tell the user when an edit arrives while Save runs (`SAVING_MESSAGE`).
const SAVING: &str = "Saving. Try that again once it finishes.";

#[derive(Clone, Debug, PartialEq)]
pub enum EditFetch {
    Preview {
        pending_rev: u64,
    },
    Save,
    /// The head read before positions are written, or for a rebase.
    Head {
        then: AfterHead,
    },
    Layout {
        changed: LayoutPatch,
    },
    Approve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterHead {
    Rebase,
    /// Positions alone save only onto the head they were moved on.
    SaveLayout,
}

#[derive(Debug)]
pub struct EditorUi {
    pub draft: Draft,
    pub saving: bool,
    pub approving: bool,
    /// The host refused the pending ops in its dry run.
    pub refusal: Option<(u64, String)>,
    /// Who changed the head while the draft was open (`proposed_by`).
    pub by: Option<String>,
    preview_due: Option<(u64, f64)>,
    pub flow: FlowState,
    pub inspected: Option<String>,
    pub fields: Fields,
    pub adding: bool,
    pub dock_open: bool,
}

impl EditorUi {
    pub fn new(detail: &Detail) -> EditorUi {
        EditorUi {
            draft: draft::open(&detail.automation, &detail.issues),
            saving: false,
            approving: false,
            refusal: None,
            by: None,
            preview_due: None,
            flow: FlowState::default(),
            inspected: None,
            fields: Fields::default(),
            adding: false,
            dock_open: false,
        }
    }

    /// The host head changed (`automation_changed`).
    pub fn head(&mut self, detail: Detail) {
        self.draft = draft::reduce(
            &self.draft,
            Action::Head {
                automation: detail.automation,
                issues: detail.issues,
            },
        );
    }

    pub fn refusal(&self) -> Option<&str> {
        self.refusal
            .as_ref()
            .filter(|(rev, _)| *rev == self.draft.pending_rev)
            .map(|(_, m)| m.as_str())
    }
}

fn message(error: &HttpError) -> (String, Option<String>) {
    match error {
        HttpError::Refused { code, message, .. } => (message.clone(), Some(code.to_lowercase())),
        HttpError::Network(detail) => (detail.clone(), None),
        HttpError::InProgress => (
            "the first run of this command is still going".to_owned(),
            None,
        ),
    }
}

impl State {
    pub fn automation_edit(&mut self) {
        let Some(detail) = self.automations.detail.data.clone() else {
            return;
        };
        log::info!("[automations] editing {}", detail.automation.id);
        self.automations.chosen = Some(detail.automation.id.clone());
        self.automations.editing = Some(detail.automation.id.clone());
        self.automations.editor = Some(EditorUi::new(&detail));
    }

    /// Close; with unsaved edits, after the discard confirm.
    pub fn automation_close_editor(&mut self) {
        let Some(editor) = &self.automations.editor else {
            return;
        };
        if editor.saving {
            return;
        }
        if editor.draft.dirty() {
            self.overlays.open(DISCARD_CONFIRM);
        } else {
            self.automations.editing = None;
            self.automations.editor = None;
        }
    }

    pub fn automation_discard_and_close(&mut self) {
        self.close(DISCARD_CONFIRM);
        self.automations.editing = None;
        self.automations.editor = None;
    }

    /// Applies a gesture; says why when it cannot.
    fn edit_apply(&mut self, ops: Vec<Op>, layout: LayoutPatch) -> bool {
        let Some(editor) = self.automations.editor.as_mut() else {
            return false;
        };
        if editor.saving {
            self.toast_later_text(SAVING);
            return false;
        }
        match draft::plan_gesture(&editor.draft, ops, layout) {
            Ok(gesture) => {
                editor.draft = draft::reduce(&editor.draft, Action::Gesture(gesture));
                editor.preview_due = Some((editor.draft.pending_rev, self.now + PREVIEW_DELAY));
                true
            }
            Err(rejection) => {
                self.toast_error(
                    &format!("The host would refuse that edit: {}", rejection.code()),
                    "",
                );
                false
            }
        }
    }

    fn toast_later_text(&mut self, words: &str) {
        self.toasts
            .push(self.now, ui::overlay::ToastKind::Info, words, None);
    }

    pub fn automation_undo(&mut self) {
        self.edit_history(Action::Undo);
    }

    pub fn automation_redo(&mut self) {
        self.edit_history(Action::Redo);
    }

    pub fn automation_discard(&mut self) {
        self.edit_history(Action::Discard);
    }

    fn edit_history(&mut self, action: Action) {
        let now = self.now;
        let Some(editor) = self.automations.editor.as_mut() else {
            return;
        };
        if editor.saving {
            return;
        }
        editor.draft = draft::reduce(&editor.draft, action);
        editor.preview_due = Some((editor.draft.pending_rev, now + PREVIEW_DELAY));
        if let Some(id) = editor.inspected.clone() {
            editor.fields = fields_of(&editor.draft, &id);
        }
    }

    /// The canvas in edit mode.
    pub fn automation_flow_event(&mut self, event: FlowEvent) {
        let Some(editor) = self.automations.editor.as_ref() else {
            return;
        };
        let (graph, layout) = (editor.draft.doc.graph.clone(), editor.draft.layout.clone());
        match event {
            FlowEvent::Select(selection) => {
                self.automation_commit_fields();
                let card = match selection {
                    Some(Selection::Card(id)) => Some(id),
                    _ => None,
                };
                self.automation_inspect(card);
            }
            FlowEvent::Moved { id, at } => {
                let moved = BTreeMap::from([(
                    id,
                    NodePosition {
                        x: f64::from(at[0]),
                        y: f64::from(at[1]),
                    },
                )]);
                if let Some(patch) = move_patch(&graph, &layout, &moved) {
                    self.edit_apply(Vec::new(), patch);
                }
            }
            FlowEvent::Connect { from, port, to } => {
                let Ok(port) = serde_json::from_value::<Port>(Value::String(port)) else {
                    return;
                };
                let edge = Edge { from, port, to };
                if let Some(refusal) = refuse_connect(&graph, &edge) {
                    self.toast_later_text(refusal.message());
                    return;
                }
                self.edit_apply(vec![Op::Connect { edge }], LayoutPatch::new());
            }
        }
    }

    pub fn automation_inspect(&mut self, card: Option<String>) {
        let Some(editor) = self.automations.editor.as_mut() else {
            return;
        };
        editor.fields = card
            .as_deref()
            .map(|id| fields_of(&editor.draft, id))
            .unwrap_or_default();
        editor.inspected = card;
    }

    /// The inspector's text into the draft, as one `update_node` or `set_trigger`.
    pub fn automation_commit_fields(&mut self) {
        let Some(editor) = self.automations.editor.as_ref() else {
            return;
        };
        let id = editor.fields.card.clone();
        if id.is_empty() || id == TRIGGER {
            if id == TRIGGER {
                let trigger = match &editor.draft.doc.trigger {
                    Trigger::Schedule {
                        schedule:
                            Schedule::Every {
                                anchor_ms,
                                every_ms,
                            },
                    } => {
                        let minutes = editor.fields.a.text.trim().parse::<f64>().ok();
                        Trigger::Schedule {
                            schedule: Schedule::Every {
                                every_ms: minutes.map_or(*every_ms, |m| (m * 60_000.0).round()),
                                anchor_ms: *anchor_ms,
                            },
                        }
                    }
                    Trigger::Schedule {
                        schedule: Schedule::Cron { .. },
                    } => Trigger::Schedule {
                        schedule: Schedule::Cron {
                            expr: editor.fields.a.text.clone(),
                            timezone: editor.fields.b.text.clone(),
                        },
                    },
                    other => other.clone(),
                };
                if trigger != editor.draft.doc.trigger {
                    self.edit_apply(vec![Op::SetTrigger { trigger }], LayoutPatch::new());
                }
            }
            return;
        }
        let Some(node) = editor.draft.doc.graph.node(&id) else {
            return;
        };
        let spec = spec_from(&node.spec, &editor.fields);
        let label = editor.fields.label.text.trim().to_owned();
        let label = (!label.is_empty() && label != node.label).then_some(label);
        let spec = (spec != node.spec).then_some(spec);
        if label.is_some() || spec.is_some() {
            self.edit_apply(vec![Op::UpdateNode { id, label, spec }], LayoutPatch::new());
        }
    }

    /// A choice the inspector makes at once (a method, an operator, a trigger kind).
    pub fn automation_set_spec(&mut self, id: &str, change: impl FnOnce(&mut NodeSpec)) {
        self.automation_commit_fields();
        let Some(node) = self
            .automations
            .editor
            .as_ref()
            .and_then(|e| e.draft.doc.graph.node(id))
        else {
            return;
        };
        let mut spec = node.spec.clone();
        change(&mut spec);
        if spec != node.spec {
            self.edit_apply(
                vec![Op::UpdateNode {
                    id: id.to_owned(),
                    label: None,
                    spec: Some(spec),
                }],
                LayoutPatch::new(),
            );
        }
    }

    pub fn automation_set_trigger(&mut self, trigger: Trigger) {
        let same = self
            .automations
            .editor
            .as_ref()
            .is_none_or(|e| e.draft.doc.trigger == trigger);
        if !same && self.edit_apply(vec![Op::SetTrigger { trigger }], LayoutPatch::new()) {
            self.automation_inspect(Some(TRIGGER.to_owned()));
        }
    }

    /// Adds a step of `kind`: after the selected card's chain port when it has a free one,
    /// else below the cards.
    pub fn automation_add(&mut self, kind: NodeKind) {
        let Some(editor) = self.automations.editor.as_mut() else {
            return;
        };
        editor.adding = false;
        let (graph, layout) = (editor.draft.doc.graph.clone(), editor.draft.layout.clone());
        let drawn = model::automations::layout::place_nodes(&graph, &layout);
        // With nothing inspected a step goes after the trigger while its output is free, as
        // the trigger card's dangling "+" adds it in React.
        let anchor = editor
            .inspected
            .clone()
            .or_else(|| Some(TRIGGER.to_owned()));
        let from = anchor.filter(|id| drawn.contains_key(id)).and_then(|id| {
            let kind_of = graph.node(&id).map(|n| n.spec.kind());
            let port = if id == TRIGGER {
                Some(Port::Out)
            } else {
                kind_of.and_then(chain_port)
            }?;
            let used = graph.edges.iter().any(|e| e.from == id && e.port == port);
            (!used).then_some((id, port))
        });
        let at = match &from {
            Some((id, _)) => {
                let p = drawn[id];
                NodePosition {
                    x: p.x + 192.0 + 80.0,
                    y: p.y,
                }
            }
            None => {
                let bottom = drawn.values().map(|p| p.y).fold(0.0, f64::max);
                let left = drawn.values().map(|p| p.x).fold(f64::INFINITY, f64::min);
                NodePosition {
                    x: if left.is_finite() { left } else { 0.0 },
                    y: bottom + 120.0,
                }
            }
        };
        let node = new_node(kind, &graph);
        let id = node.id.clone();
        let (ops, patch) = add_gesture(
            &graph,
            &layout,
            &node,
            chain_port(kind),
            &AddPlacement {
                at,
                from,
                split: None,
            },
        );
        if self.edit_apply(ops, patch) {
            if let Some(editor) = self.automations.editor.as_mut() {
                editor.flow.selection = Some(Selection::Card(id.clone()));
            }
            self.automation_inspect(Some(id));
        }
    }

    /// Deletes the selected card or edge; React's editor asks nothing (Undo brings it back).
    pub fn automation_delete_selected(&mut self) {
        let Some(editor) = self.automations.editor.as_ref() else {
            return;
        };
        let (graph, layout) = (editor.draft.doc.graph.clone(), editor.draft.layout.clone());
        let (nodes, edges): (Vec<String>, Vec<Edge>) = match &editor.flow.selection {
            Some(Selection::Card(id)) if id != TRIGGER => (vec![id.clone()], Vec::new()),
            Some(Selection::Wire(key)) => (
                Vec::new(),
                graph
                    .edges
                    .iter()
                    .filter(|e| e.key() == *key)
                    .cloned()
                    .collect(),
            ),
            _ => return,
        };
        let (ops, patch) = remove_gesture(&graph, &layout, &nodes, &edges);
        if !ops.is_empty() && self.edit_apply(ops, patch) {
            if let Some(editor) = self.automations.editor.as_mut() {
                editor.flow.selection = None;
            }
            self.automation_inspect(None);
        }
    }

    /// Sends the dry run once the gestures settle.
    pub(crate) fn automation_editor_tick(&mut self, now: f64) -> Option<f64> {
        let editor = self.automations.editor.as_mut()?;
        let (rev, due) = editor.preview_due?;
        if rev != editor.draft.pending_rev {
            editor.preview_due = None;
            return None;
        }
        if now < due {
            return Some(due);
        }
        editor.preview_due = None;
        let ops = editor.draft.pending_ops();
        if ops.is_empty() || editor.saving {
            return None;
        }
        let base = &editor.draft.base;
        let call = Call::PreviewOps {
            id: base.id.clone(),
            base_revision: base.revision,
            ops,
        };
        self.automation_send(
            call,
            AutoFetch::Edit(EditFetch::Preview { pending_rev: rev }),
        );
        None
    }

    pub fn automation_save(&mut self) {
        self.automation_commit_fields();
        let Some(editor) = self.automations.editor.as_mut() else {
            return;
        };
        if editor.saving || !editor.draft.dirty() {
            return;
        }
        editor.saving = true;
        let base = editor.draft.base.clone();
        let ops = editor.draft.pending_ops();
        if ops.is_empty() {
            self.automation_send(
                Call::Get { id: base.id },
                AutoFetch::Edit(EditFetch::Head {
                    then: AfterHead::SaveLayout,
                }),
            );
        } else {
            self.automation_send(
                Call::ApplyOps {
                    id: base.id,
                    base_revision: base.revision,
                    ops,
                },
                AutoFetch::Edit(EditFetch::Save),
            );
        }
    }

    pub fn automation_editor_approve(&mut self) {
        let Some(editor) = self.automations.editor.as_mut() else {
            return;
        };
        editor.approving = true;
        let (id, revision) = (editor.draft.base.id.clone(), editor.draft.base.revision);
        self.automation_send(
            Call::Approve { id, revision },
            AutoFetch::Edit(EditFetch::Approve),
        );
    }

    /// The positions this draft moved, onto `head`'s layout.
    fn save_layout(&mut self, head: &Automation) {
        let Some(editor) = self.automations.editor.as_ref() else {
            return;
        };
        let changed = draft::changed_positions(&editor.draft.base.layout, &editor.draft.layout);
        if changed.is_empty() {
            self.edit_saved(head.clone(), None);
            return;
        }
        let layout = draft::patch_layout(&head.layout, &changed);
        self.automation_send(
            Call::SetLayout {
                id: head.id.clone(),
                layout,
            },
            AutoFetch::Edit(EditFetch::Layout { changed }),
        );
    }

    fn edit_saved(&mut self, automation: Automation, requeue: Option<LayoutPatch>) {
        let Some(editor) = self.automations.editor.as_mut() else {
            return;
        };
        let issues = editor.draft.issues.clone();
        log::info!(
            "[automations] saved {} ops on {}: revision {}",
            editor.draft.pending_ops().len(),
            automation.id,
            automation.revision
        );
        editor.draft = draft::reduce(&editor.draft, Action::Saved { automation, issues });
        if let Some(layout) = requeue {
            editor.draft = draft::reduce(&editor.draft, Action::Requeue(layout));
        }
        editor.saving = false;
        self.automations_reload();
    }

    fn rebase(&mut self) {
        let Some(editor) = self.automations.editor.as_ref() else {
            return;
        };
        let id = editor.draft.base.id.clone();
        self.automation_send(
            Call::Get { id },
            AutoFetch::Edit(EditFetch::Head {
                then: AfterHead::Rebase,
            }),
        );
    }

    pub(crate) fn automation_edit_replied(
        &mut self,
        tag: EditFetch,
        result: Result<Value, HttpError>,
    ) {
        if self.automations.editor.is_none() {
            return;
        }
        match tag {
            EditFetch::Preview { pending_rev } => {
                let current = self
                    .automations
                    .editor
                    .as_ref()
                    .is_some_and(|e| e.draft.pending_rev == pending_rev && !e.saving);
                if !current {
                    return;
                }
                match result {
                    Ok(data) => match serde_json::from_value::<Detail>(data) {
                        Ok(detail) => {
                            let editor = self.automations.editor.as_mut().expect("editor");
                            if detail.automation.graph != editor.draft.doc.graph {
                                log::warn!("[automations] local op mirror drifted from the host");
                            }
                            editor.refusal = None;
                            editor.draft = draft::reduce(
                                &editor.draft,
                                Action::Previewed {
                                    pending_rev,
                                    issues: detail.issues,
                                },
                            );
                        }
                        Err(e) => log::warn!(
                            "[automations] preview_ops returned an unexpected shape: {e}"
                        ),
                    },
                    Err(e) => {
                        let (text, code) = message(&e);
                        if code.as_deref() == Some("revision_conflict") {
                            self.rebase();
                        } else {
                            log::warn!("[automations] dry run refused: {text}");
                            let editor = self.automations.editor.as_mut().expect("editor");
                            editor.refusal = Some((pending_rev, text));
                        }
                    }
                }
            }
            EditFetch::Save => match result {
                Ok(data) => match serde_json::from_value::<ApplyOpsResult>(data) {
                    Ok(saved) => {
                        if let Some(editor) = self.automations.editor.as_mut() {
                            editor.draft.issues = saved.issues.clone();
                        }
                        self.save_layout(&saved.automation);
                    }
                    Err(e) => self.edit_failed(&format!("Could not save: unexpected reply: {e}")),
                },
                Err(e) => {
                    let (text, code) = message(&e);
                    if code.as_deref() == Some("revision_conflict") {
                        if let Some(editor) = self.automations.editor.as_mut() {
                            editor.saving = false;
                        }
                        self.rebase();
                    } else {
                        self.edit_failed(&format!("Could not save: {text}"));
                    }
                }
            },
            EditFetch::Head { then } => {
                let head: Result<Detail, String> = decode("get", result);
                match (then, head) {
                    (_, Err(e)) => {
                        self.edit_failed(&format!("Could not load the latest version: {e}"))
                    }
                    (AfterHead::Rebase, Ok(head)) => {
                        let editor = self.automations.editor.as_mut().expect("editor");
                        let by = editor.by.clone();
                        editor.saving = false;
                        editor.draft = draft::reduce(
                            &editor.draft,
                            Action::Rebased {
                                automation: head.automation,
                                issues: head.issues,
                                by,
                            },
                        );
                    }
                    (AfterHead::SaveLayout, Ok(head)) => {
                        let base = self
                            .automations
                            .editor
                            .as_ref()
                            .map(|e| e.draft.base.revision);
                        if Some(head.automation.revision) != base {
                            if let Some(editor) = self.automations.editor.as_mut() {
                                editor.saving = false;
                            }
                            self.rebase();
                        } else {
                            self.save_layout(&head.automation);
                        }
                    }
                }
            }
            EditFetch::Layout { changed } => match decode::<Automation>("set_layout", result) {
                Ok(automation) => self.edit_saved(automation, None),
                Err(e) => {
                    self.toast_error(&format!("Saved, but the positions were not: {e}"), "");
                    let base = self
                        .automations
                        .editor
                        .as_ref()
                        .map(|e| e.draft.base.clone());
                    if let Some(base) = base {
                        self.edit_saved(base, Some(changed));
                    }
                }
            },
            EditFetch::Approve => {
                if let Some(editor) = self.automations.editor.as_mut() {
                    editor.approving = false;
                }
                if let Err(e) = result {
                    self.toast_error(&format!("Could not approve: {}", message(&e).0), "");
                }
                self.automations_reload();
            }
        }
    }

    fn edit_failed(&mut self, text: &str) {
        if let Some(editor) = self.automations.editor.as_mut() {
            editor.saving = false;
        }
        self.toast_error(text, "");
    }
}
